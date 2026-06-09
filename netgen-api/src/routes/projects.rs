use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    db::schema::ProjectRow,
    middleware::auth::Claims,
    AppState,
};

// ─── Response shapes ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ProjectResponse {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ProjectRow> for ProjectResponse {
    fn from(r: ProjectRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            status: r.status,
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
        }
    }
}

/// Detailed project response — includes graph/intent data.
/// Returned only by GET /api/projects/:id.
/// Excludes `user_id` to avoid leaking internal UUIDs.
#[derive(Serialize)]
pub struct ProjectDetailResponse {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub intent_json: Option<serde_json::Value>,
    pub graph_json: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ProjectRow> for ProjectDetailResponse {
    fn from(r: ProjectRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            status: r.status,
            intent_json: r.intent_json,
            graph_json: r.graph_json,
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
        }
    }
}

/// Sanitizes a user-supplied name for use in HTTP `Content-Disposition` filenames.
/// Keeps only alphanumerics, hyphens, underscores, and dots; caps at 100 chars.
fn safe_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .take(100)
        .collect()
}

// ─── POST /api/projects ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
}

pub async fn create_project(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Json(req): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<ProjectResponse>), (StatusCode, Json<serde_json::Value>)> {
    let pool = state.db.as_ref().ok_or_else(db_unavailable)?;

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "project name must not be empty"})),
        ));
    }
    if name.len() > 128 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "project name must not exceed 128 characters"})),
        ));
    }

    let user_id = claims.user_id().ok_or_else(|| internal_error("invalid user id in token"))?;

    // Enforce project limits per tier
    let max = claims.tier.max_projects();
    if max > 0 {
        let count = crate::db::projects::count_for_user(pool, user_id)
            .await
            .map_err(db_error)? as usize;
        if count >= max {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": format!("project limit reached ({max}) — upgrade to Pro or Enterprise")
                })),
            ));
        }
    }

    let project = crate::db::projects::create(pool, user_id, &name)
        .await
        .map_err(db_error)?;

    Ok((StatusCode::CREATED, Json(project.into())))
}

// ─── GET /api/projects ─────────────────────────────────────────────────────────

pub async fn list_projects(
    State(state): State<Arc<AppState>>,
    claims: Claims,
) -> Result<Json<Vec<ProjectResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let pool = state.db.as_ref().ok_or_else(db_unavailable)?;
    let user_id = claims.user_id().ok_or_else(|| internal_error("invalid user id in token"))?;

    let projects = crate::db::projects::list(pool, user_id)
        .await
        .map_err(db_error)?;

    Ok(Json(projects.into_iter().map(Into::into).collect()))
}

// ─── GET /api/projects/:id ─────────────────────────────────────────────────────

pub async fn get_project(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<ProjectDetailResponse>, (StatusCode, Json<serde_json::Value>)> {
    let pool = state.db.as_ref().ok_or_else(db_unavailable)?;
    let user_id = claims.user_id().ok_or_else(|| internal_error("invalid user id in token"))?;

    let project = crate::db::projects::get(pool, id, user_id)
        .await
        .map_err(db_error)?
        .ok_or_else(not_found)?;

    Ok(Json(project.into()))
}

// ─── DELETE /api/projects/:id ──────────────────────────────────────────────────

pub async fn delete_project(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let pool = state.db.as_ref().ok_or_else(db_unavailable)?;
    let user_id = claims.user_id().ok_or_else(|| internal_error("invalid user id in token"))?;

    let deleted = crate::db::projects::delete(pool, id, user_id)
        .await
        .map_err(db_error)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

// ─── GET /api/projects/:id/export/:format ────────────────────────────────────

pub async fn export_project(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path((id, format)): Path<(Uuid, String)>,
) -> Result<(StatusCode, axum::response::Response), (StatusCode, Json<serde_json::Value>)> {
    let pool = state.db.as_ref().ok_or_else(db_unavailable)?;
    let user_id = claims.user_id().ok_or_else(|| internal_error("invalid user id in token"))?;

    let project = crate::db::projects::get(pool, id, user_id)
        .await
        .map_err(db_error)?
        .ok_or_else(not_found)?;

    let graph_json = project.graph_json.ok_or_else(|| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error": "project has no generated graph yet — run generation first"})),
        )
    })?;

    use axum::{body::Body, response::Response};
    use netgen_core::{export::*, graph::TopologyGraph};

    let graph: TopologyGraph = serde_json::from_value(graph_json).map_err(|e| {
        internal_error(&format!("failed to deserialize graph: {e}"))
    })?;

    match format.as_str() {
        "gns3" => {
            let bytes = Gns3Exporter.export(&graph)
                .map_err(|e| internal_error(&e.to_string()))?;
            let resp = Response::builder()
                .status(200)
                .header("Content-Type", "application/zip")
                .header(
                    "Content-Disposition",
                    format!("attachment; filename=\"{}.gns3project\"", safe_filename(&project.name)),
                )
                .body(Body::from(bytes))
                .unwrap();
            Ok((StatusCode::OK, resp))
        }
        "svg" => {
            let svg = DiagramExporter.to_svg(&graph)
                .map_err(|e| internal_error(&e.to_string()))?;
            let resp = Response::builder()
                .status(200)
                .header("Content-Type", "image/svg+xml")
                .header(
                    "Content-Disposition",
                    format!("inline; filename=\"{}.svg\"", safe_filename(&project.name)),
                )
                .body(Body::from(svg))
                .unwrap();
            Ok((StatusCode::OK, resp))
        }
        "csv" => {
            let csv = IpPlanExporter.to_csv(&graph)
                .map_err(|e| internal_error(&e.to_string()))?;
            let resp = Response::builder()
                .status(200)
                .header("Content-Type", "text/csv")
                .header(
                    "Content-Disposition",
                    format!("attachment; filename=\"{}_ip_plan.csv\"", safe_filename(&project.name)),
                )
                .body(Body::from(csv))
                .unwrap();
            Ok((StatusCode::OK, resp))
        }
        "docs" => {
            let md = DocExporter.to_markdown(&graph)
                .map_err(|e| internal_error(&e.to_string()))?;
            let resp = Response::builder()
                .status(200)
                .header("Content-Type", "text/markdown")
                .header(
                    "Content-Disposition",
                    format!("attachment; filename=\"{}_design.md\"", safe_filename(&project.name)),
                )
                .body(Body::from(md))
                .unwrap();
            Ok((StatusCode::OK, resp))
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "unknown format — use gns3, svg, csv, or docs"})),
        )),
    }
}

// ─── GET /api/projects/:id/turns ──────────────────────────────────────────────

#[derive(Serialize)]
pub struct ConversationTurnResponse {
    pub id: Uuid,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

pub async fn get_project_turns(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ConversationTurnResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let pool = state.db.as_ref().ok_or_else(db_unavailable)?;
    let user_id = claims.user_id().ok_or_else(|| internal_error("invalid user id in token"))?;

    let turns = crate::db::conversation_turns::list_by_project(pool, id, user_id)
        .await
        .map_err(db_error)?;

    Ok(Json(
        turns
            .into_iter()
            .map(|t| ConversationTurnResponse {
                id: t.id,
                role: t.role,
                content: t.content,
                created_at: t.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn db_unavailable() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"error": "database not configured"})),
    )
}

fn db_error(e: sqlx::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("database error: {e}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "database error"})),
    )
}

fn internal_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("internal error: {msg}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "internal server error"})),
    )
}

fn not_found() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "project not found"})),
    )
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<AppState>> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/", post(create_project).get(list_projects))
        .route("/{id}", get(get_project).delete(delete_project))
        .route("/{id}/turns", get(get_project_turns))
        .route("/{id}/export/{format}", get(export_project))
}
