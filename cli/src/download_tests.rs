use super::*;
use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;
type PackageState = Arc<Mutex<(Value, BTreeMap<String, Vec<u8>>)>>;
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn package() -> (Value, BTreeMap<String, Vec<u8>>) {
    let mut files = BTreeMap::new();
    let mut entries = serde_json::Map::new();
    for name in [
        "bom.json",
        "catalog.json",
        "decisions.json",
        "firewall.json",
        "ipam.json",
        "options.json",
        "provenance.json",
        "requirements.json",
        "routing.json",
        "selection.json",
        "topology.json",
        "verification.json",
    ] {
        let bytes = format!("{{\"name\":\"{name}\"}}").into_bytes();
        entries.insert(
            name.to_string(),
            json!({"sha256":hash(&bytes),"size_bytes":bytes.len()}),
        );
        files.insert(name.to_string(), bytes);
    }
    let manifest =
        serde_json::to_vec(&json!({"schema_version":"engineering_package_v1","files":entries}))
            .unwrap();
    let manifest_hash = hash(&manifest);
    files.insert("manifest.json".into(), manifest);
    let artifacts: Vec<_> = files
        .iter()
        .map(|(name, bytes)| json!({"name":name,"sha256":hash(bytes),"size_bytes":bytes.len()}))
        .collect();
    (
        json!({"revision_id":"33333333-3333-4333-8333-333333333333","manifest_sha256":manifest_hash,"artifacts":artifacts}),
        files,
    )
}
#[tokio::test]
async fn download_verifies_complete_inventory_before_private_output_and_refuses_overwrite() {
    let (mut client, _, auth_task) = tests::provider().await;
    let vault = tests::MemoryVault::default();
    tests::signed_in(&client, &vault).await;
    let (metadata, files) = package();
    let routes = Arc::new(Mutex::new((metadata, files)));
    async fn serve(
        State(state): State<PackageState>,
        Path(path): Path<String>,
        headers: HeaderMap,
    ) -> axum::response::Response {
        assert_eq!(headers["authorization"], "Bearer access-secret");
        let state = state.lock().unwrap();
        let bytes = if path.contains("/artifacts/") {
            state
                .1
                .get(path.rsplit('/').next().unwrap())
                .unwrap()
                .clone()
        } else {
            serde_json::to_vec(&state.0).unwrap()
        };
        (StatusCode::OK, bytes).into_response()
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    client.config.api =
        reqwest::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let app = Router::new()
        .route("/{*path}", get(serve))
        .with_state(routes.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let base = std::env::temp_dir().join(format!(
        "laby-download-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&base).unwrap();
    let out = base.join("package");
    let id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
    client.download(&vault, id, id, id, &out).await.unwrap();
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 13);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&out).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(out.join("manifest.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert_eq!(
        client.download(&vault, id, id, id, &out).await,
        Err(Error::Output)
    );
    let bad = base.join("bad");
    routes
        .lock()
        .unwrap()
        .1
        .insert("bom.json".into(), b"{}".to_vec());
    assert!(client.download(&vault, id, id, id, &bad).await.is_err());
    assert!(!bad.exists());
    let (meta, original) = package();
    *routes.lock().unwrap() = (meta, original);
    routes.lock().unwrap().0["artifacts"][0]["name"] = json!("../escape.json");
    assert_eq!(
        client.download(&vault, id, id, id, &bad).await,
        Err(Error::Integrity)
    );
    assert!(!bad.exists());
    // Invalid authoritative inventory must fail before any output is created.
    for case in 0..7 {
        let (mut meta, original) = package();
        match case {
            0 => meta["manifest_sha256"] = json!("0".repeat(64)),
            1 => meta["revision_id"] = json!("44444444-4444-4444-8444-444444444444"),
            2 => meta["artifacts"][0]["size_bytes"] = json!(33 * 1024 * 1024),
            3 => meta["artifacts"][0]["sha256"] = json!("bad-hash"),
            4 => meta["artifacts"][0] = meta["artifacts"][1].clone(),
            5 => {
                meta["artifacts"].as_array_mut().unwrap().pop();
            }
            _ => meta["artifacts"][0]["size_bytes"] = json!(0),
        }
        *routes.lock().unwrap() = (meta, original);
        assert_eq!(
            client.download(&vault, id, id, id, &bad).await,
            Err(Error::Integrity)
        );
        assert!(!bad.exists());
    }
    std::fs::remove_dir_all(base).unwrap();
    task.abort();
    auth_task.abort();
}
