use super::*;
use crate::auth::test_support::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use tower::ServiceExt;
#[tokio::test]
async fn missing_or_invalid_tokens_never_reach_storage_and_errors_are_private() {
    let source = TestIssuer::start().await;
    let verifier = Arc::new(source.verifier().await.unwrap());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    let app = router(
        pool,
        verifier,
        HttpLimits {
            max_in_flight: 2,
            request_timeout: Duration::from_secs(1),
        },
    )
    .unwrap();
    for token in [None, Some("Bearer invalid"), Some("Basic invalid")] {
        let mut request=Request::builder().uri("/v1/tenants/00000000-0000-0000-0000-000000000001/projects/00000000-0000-0000-0000-000000000002/jobs/00000000-0000-0000-0000-000000000003");
        if let Some(token) = token {
            request = request.header("authorization", token);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
        assert!(response.headers().contains_key("x-request-id"));
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["error"]["code"], "unauthorized");
    }
}

#[tokio::test]
async fn malformed_duplicate_and_oversized_requests_have_bounded_structured_errors() {
    let source = TestIssuer::start().await;
    let auth = Arc::new(source.verifier().await.unwrap());
    let token = source.signer.token(&claims(Uuid::new_v4()));
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused@127.0.0.1:1/unused")
        .unwrap();
    let app = router(
        pool,
        auth,
        HttpLimits {
            max_in_flight: 2,
            request_timeout: Duration::from_secs(1),
        },
    )
    .unwrap();
    let path = format!(
        "/v1/tenants/{}/projects/{}/jobs",
        Uuid::new_v4(),
        Uuid::new_v4()
    );
    for body in ["not json".to_string(),r#"{"intent":{},"catalog_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#.into(),r#"{"intent":{},"catalog_sha256":"a","expected_head_revision_id":null,"actor_id":"spoofed"}"#.into(),r#"{"intent":{},"intent":{},"expected_head_revision_id":null,"catalog_sha256":"a"}"#.into()] {
        let response=app.clone().oneshot(Request::builder().method("POST").uri(&path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").header("idempotency-key","key").body(Body::from(body)).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::BAD_REQUEST);let value:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),4096).await.unwrap()).unwrap();assert_eq!(value["error"]["code"],"invalid_request");
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .header("idempotency-key", "key")
                .body(Body::from(vec![b'x'; MAX_BODY + 1]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let value: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(value["error"]["code"], "payload_too_large");
    let response = app
        .oneshot(
            Request::builder()
                .uri(&path)
                .header("authorization", format!("Bearer {token}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
#[tokio::test]
async fn request_capacity_and_deadline_are_enforced_without_waiting_for_storage() {
    let source = TestIssuer::start().await;
    let auth = Arc::new(source.verifier().await.unwrap());
    let token = source.signer.token(&claims(Uuid::new_v4()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(3))
        .connect_lazy(&format!(
            "postgres://unused@{}/unused?sslmode=disable",
            listener.local_addr().unwrap()
        ))
        .unwrap();
    let app = router(
        pool,
        auth,
        HttpLimits {
            max_in_flight: 1,
            request_timeout: Duration::from_secs(1),
        },
    )
    .unwrap();
    let path = format!(
        "/v1/tenants/{}/projects/{}/jobs/{}",
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4()
    );
    let request = || {
        Request::builder()
            .uri(&path)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    let first = tokio::spawn(app.clone().oneshot(request()));
    let (connection, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let response = app.oneshot(request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(body["error"]["code"], "server_busy");
    let response = first.await.unwrap().unwrap();
    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    drop(connection);
}

#[tokio::test]
async fn liveness_exposes_no_private_data_and_does_not_bypass_business_authentication() {
    let source = TestIssuer::start().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused@127.0.0.1:1/unused")
        .unwrap();
    let app = router(
        pool,
        Arc::new(source.verifier().await.unwrap()),
        HttpLimits {
            max_in_flight: 2,
            request_timeout: Duration::from_secs(1),
        },
    )
    .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        to_bytes(response.into_body(), 1024)
            .await
            .unwrap()
            .is_empty()
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health/live/private")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
