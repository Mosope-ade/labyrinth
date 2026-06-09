// Tauri command bridge: frontend (netgen-web) ↔ netgen-core (direct, no HTTP server).
// All pipeline stages run in this process — offline mode is a first-class citizen.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use tauri::Manager as _;
use netgen_api::ai::{AiClient, ConversationEngine, DesignPhase, Industry};
use netgen_core::{
    addressing::AddressingEngine,
    export::{DiagramExporter, DocExporter, Gns3Exporter, IpPlanExporter},
    graph::TopologyGraph,
    labelling::LabellingEngine,
    rule_engine::RuleEngine,
    security::SecurityEngine,
    templates::{ConfigGenerator, select_adapter},
    verification::VerificationEngine,
};

// ─── Desktop state ────────────────────────────────────────────────────────────

pub struct DesktopState {
    engine: ConversationEngine,
    /// Completed graphs keyed by session ID (populated after pipeline runs).
    graphs: Mutex<HashMap<Uuid, TopologyGraph>>,
    /// Local project storage directory (app_data_dir/projects/).
    projects_dir: PathBuf,
}

impl DesktopState {
    fn new(engine: ConversationEngine, app_data_dir: PathBuf) -> Self {
        let projects_dir = app_data_dir.join("projects");
        std::fs::create_dir_all(&projects_dir).ok();
        Self {
            engine,
            graphs: Mutex::new(HashMap::new()),
            projects_dir,
        }
    }
}

// ─── DTOs ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ChatTurnResponse {
    pub session_id: String,
    pub phase: DesignPhase,
    pub reply: String,
    pub intent_ready: bool,
    pub fallback_required: bool,
    pub detected_industry: Option<Industry>,
}

#[derive(Serialize, Deserialize)]
pub struct LocalProject {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub graph_json: serde_json::Value,
}

#[derive(Serialize)]
pub struct PipelineError {
    pub stage: String,
    pub message: String,
}

// ─── Pipeline ─────────────────────────────────────────────────────────────────

/// Runs the full netgen-core pipeline and returns the final graph.
/// Runs synchronously — called from within a tokio async task so it's fine.
fn run_pipeline(
    intent: &netgen_core::schema::NetworkIntent,
) -> Result<TopologyGraph, PipelineError> {
    // 1. Rule engine → topology graph
    let rule_engine = RuleEngine::new();
    let chain = rule_engine
        .evaluate(intent)
        .map_err(|e| PipelineError { stage: "rule_engine".into(), message: e.to_string() })?;
    let mut graph = chain.latest_graph().clone();

    // 2. Security policy gate
    let security = SecurityEngine::new();
    let (secured_graph, warnings) = security
        .validate(&graph, intent)
        .map_err(|e| PipelineError { stage: "security_policy".into(), message: e.to_string() })?;
    graph = secured_graph;

    for w in &warnings {
        tracing::warn!(rule_id = %w.rule_id, "{}", w.message);
    }

    // 3. Addressing
    let addressing = AddressingEngine;
    addressing
        .allocate(&mut graph, intent)
        .map_err(|e| PipelineError { stage: "addressing".into(), message: e.to_string() })?;

    // 4. Config templates — adapter selected from intent vendor_preference
    let adapter = select_adapter(&intent.metadata.vendor_preference);
    let generator = ConfigGenerator;
    let objects = generator
        .generate(&graph, &intent.routing, &intent.metadata.security_level, adapter.as_ref())
        .map_err(|e| PipelineError { stage: "config_templates".into(), message: e.to_string() })?;

    // Stitch per-node configs
    let node_ids: Vec<Uuid> = graph.nodes.iter().map(|n| n.id).collect();
    for node_id in node_ids {
        let stitched = ConfigGenerator::stitch_device_config(&objects, node_id);
        if !stitched.is_empty() {
            if let Some(node) = graph.node_mut(node_id) {
                node.config = Some(netgen_core::graph::DeviceConfig {
                    raw: stitched,
                    template_version: "cisco-ios-v1".into(),
                });
            }
        }
    }

    // 5. Labelling
    let labeller = LabellingEngine;
    labeller.apply(&mut graph);

    // 6. Verification (non-blocking in desktop/offline mode — log failures, don't abort)
    let verifier = VerificationEngine;
    match verifier.verify(&graph, intent) {
        Ok(report) => {
            tracing::info!("Verification passed: {} checks", report.passed.len());
        }
        Err(failures) => {
            for f in &failures {
                tracing::warn!(check = %f.check, "{}", f.message);
            }
        }
    }

    Ok(graph)
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

/// One conversation turn. When the AI produces a valid intent, the pipeline
/// runs inline and the graph is stored for retrieval via `get_graph`.
#[tauri::command]
async fn chat_turn(
    message: String,
    session_id: Option<String>,
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<ChatTurnResponse, String> {
    if message.trim().is_empty() {
        return Err("message must not be empty".into());
    }

    let sid = session_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());

    let result = state
        .engine
        .turn(sid, &message)
        .await
        .map_err(|e| e.to_string())?;

    let mut intent_ready = false;

    if let Some(parsed) = result.intent {
        match run_pipeline(&parsed.intent) {
            Ok(graph) => {
                state.graphs.lock().await.insert(result.session_id, graph);
                intent_ready = true;
            }
            Err(e) => {
                tracing::error!(stage = %e.stage, "Pipeline failed: {}", e.message);
                return Err(format!("Pipeline failed at {}: {}", e.stage, e.message));
            }
        }
    }

    Ok(ChatTurnResponse {
        session_id: result.session_id.to_string(),
        phase: result.phase,
        reply: result.reply,
        intent_ready,
        fallback_required: result.fallback_required,
        detected_industry: result.detected_industry,
    })
}

/// Returns the compiled graph for a given session, or null if not yet built.
#[tauri::command]
async fn get_graph(
    session_id: String,
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<Option<TopologyGraph>, String> {
    let sid = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    let graphs = state.graphs.lock().await;
    Ok(graphs.get(&sid).cloned())
}

/// Export the graph for a session as text content (SVG, CSV, or Markdown).
#[tauri::command]
async fn export_content(
    session_id: String,
    format: String,
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<String, String> {
    let sid = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    let graphs = state.graphs.lock().await;
    let graph = graphs
        .get(&sid)
        .ok_or_else(|| "Graph not found for session".to_string())?;

    match format.as_str() {
        "svg" => DiagramExporter.to_svg(graph).map_err(|e| e.to_string()),
        "csv" => IpPlanExporter.to_csv(graph).map_err(|e| e.to_string()),
        "md"  => DocExporter.to_markdown(graph).map_err(|e| e.to_string()),
        _ => Err(format!("Unknown export format: {format}")),
    }
}

/// Export GNS3 archive bytes — returns base64-encoded ZIP.
#[tauri::command]
async fn export_gns3(
    session_id: String,
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<String, String> {
    let sid = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    let graphs = state.graphs.lock().await;
    let graph = graphs
        .get(&sid)
        .ok_or_else(|| "Graph not found for session".to_string())?;

    let bytes = Gns3Exporter.export(graph).map_err(|e| e.to_string())?;
    Ok(base64_encode(&bytes))
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
        out.push(ALPHABET[b0 >> 2] as char);
        out.push(ALPHABET[((b0 & 3) << 4) | (b1 >> 4)] as char);
        out.push(if chunk.len() > 1 { ALPHABET[((b1 & 15) << 2) | (b2 >> 6)] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[b2 & 63] as char } else { '=' });
    }
    out
}

// ─── Local project storage ────────────────────────────────────────────────────

/// List all locally saved projects.
#[tauri::command]
fn list_projects(
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<Vec<LocalProject>, String> {
    let dir = &state.projects_dir;
    let mut projects = Vec::new();

    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(p) = serde_json::from_str::<LocalProject>(&content) {
                    projects.push(p);
                }
            }
        }
    }

    projects.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(projects)
}

/// Save a project locally. Returns the saved project metadata.
#[tauri::command]
fn save_project(
    name: String,
    graph_json: String,
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<LocalProject, String> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let graph_value: serde_json::Value =
        serde_json::from_str(&graph_json).map_err(|e| e.to_string())?;

    let project = LocalProject {
        id: id.clone(),
        name,
        created_at: now,
        graph_json: graph_value,
    };

    let path = state.projects_dir.join(format!("{id}.json"));
    let content = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;

    Ok(project)
}

/// Load a single project by ID.
#[tauri::command]
fn load_project(
    id: String,
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<Option<LocalProject>, String> {
    let path = state.projects_dir.join(format!("{id}.json"));
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let project = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    Ok(Some(project))
}

/// Delete a project by ID.
#[tauri::command]
fn delete_project(
    id: String,
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<bool, String> {
    let path = state.projects_dir.join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

// ─── App entry point ──────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load .env if present (dev convenience)
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "netgen_desktop=info,netgen_api=info,netgen_core=info".into()),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle();

            let app_data_dir = handle
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");

            std::fs::create_dir_all(&app_data_dir)
                .expect("failed to create app data dir");

            let client = AiClient::from_env()
                .expect("AI_API_KEY must be set — set it in your environment or a .env file");

            let engine = ConversationEngine::new(client);
            let desktop_state = Arc::new(DesktopState::new(engine, app_data_dir));
            handle.manage(desktop_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            chat_turn,
            get_graph,
            export_content,
            export_gns3,
            list_projects,
            save_project,
            load_project,
            delete_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
