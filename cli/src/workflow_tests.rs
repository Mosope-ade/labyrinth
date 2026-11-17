use super::*;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn structured_submission_preserves_duplicate_fields_and_key_without_retry() {
    let (mut client, _, auth) = tests::provider().await;
    let vault = tests::MemoryVault::default();
    tests::signed_in(&client, &vault).await;
    let received = Arc::new(Mutex::new(Vec::new()));
    async fn receive(
        State(s): State<Arc<Mutex<Vec<Vec<u8>>>>>,
        headers: HeaderMap,
        body: Bytes,
    ) -> (StatusCode, Json<Value>) {
        assert_eq!(headers["authorization"], "Bearer access-secret");
        assert_eq!(headers["idempotency-key"], "stable-intent-001");
        assert_eq!(headers["content-type"], "application/json");
        s.lock().unwrap().push(body.to_vec());
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(
                json!({"error":{"code":"needs_clarification","message":"not echoed","details":{"fields":[{"path":"sites","reason":"missing\u{001b}[31m"}]}}}),
            ),
        )
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    client.config.api =
        reqwest::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let app = Router::new()
        .route("/v1/submit", post(receive))
        .with_state(received.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let raw = br#"{"intent":{},"intent":{"duplicate":true}}"#.to_vec();
    let error = client
        .submit(&vault, "v1/submit", raw.clone(), Some("stable-intent-001"))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::ApiDetails { status: 422, .. }));
    let display = error.to_string();
    assert!(display.contains("needs_clarification"));
    assert!(!display.contains('\u{001b}'));
    assert!(!display.contains("not echoed"));
    assert_eq!(*received.lock().unwrap(), vec![raw]);
    server.abort();
    auth.abort();
}
#[tokio::test]
async fn pagination_parameters_are_encoded_and_invalid_requests_do_not_authenticate() {
    let (mut client, p, auth) = tests::provider().await;
    let vault = tests::MemoryVault::default();
    for limit in [0, 101] {
        assert_eq!(
            client.page(&vault, "v1/items", limit, None).await,
            Err(Error::InvalidInput)
        );
    }
    assert_eq!(
        client
            .submit(&vault, "https://evil.example", b"{}".to_vec(), None)
            .await,
        Err(Error::InvalidInput)
    );
    assert_eq!(
        client
            .submit(&vault, "v1/submit", vec![b' '; 2 * 1024 * 1024 + 1], None)
            .await,
        Err(Error::InvalidInput)
    );
    assert_eq!(
        client
            .page(&vault, "v1/items", 25, Some("bad&cursor"))
            .await,
        Err(Error::InvalidInput)
    );
    assert!(p.events.lock().unwrap().is_empty());
    tests::signed_in(&client, &vault).await;
    async fn page(Query(query): Query<BTreeMap<String, String>>) -> Json<Value> {
        assert_eq!(query.get("limit").unwrap(), "50");
        assert_eq!(query.get("after").unwrap(), "abcdef");
        Json(json!({"items":[],"next_after":null}))
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    client.config.api =
        reqwest::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let app = Router::new().route("/v1/items", get(page));
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    assert_eq!(
        client
            .page(&vault, "v1/items", 50, Some("abcdef"))
            .await
            .unwrap()["items"],
        json!([])
    );
    task.abort();
    auth.abort();
}
