use super::*;
use axum::{
    Json, Router,
    extract::Query,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
#[tokio::test]
async fn login_refresh_rotation_and_concurrent_callers_are_coalesced() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app=Router::new().route("/auth/v1/token",post(move |headers:HeaderMap,Query(query):Query<HashMap<String,String>>,Json(body):Json<Value>|{let seen=seen.clone();async move{
  assert_eq!(headers["apikey"],"sb_publishable_test");let n=seen.fetch_add(1,Ordering::SeqCst);
  if n==0 {assert_eq!(query["grant_type"],"password");assert_eq!(body["password"],"technical-secret");}else{assert_eq!(query["grant_type"],"refresh_token");assert_eq!(body["refresh_token"],"refresh-0");assert!(body.get("password").is_none());}
  Json(json!({"access_token":format!("access-{n}"),"refresh_token":format!("refresh-{n}"),"expires_in":1}))
 }}));
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut auth = Credentials::password(
        "https://example.com",
        "sb_publishable_test".into(),
        "worker@example.com".into(),
        "technical-secret".into(),
    )
    .unwrap();
    let Mode::Technical(account) = &mut auth.mode else {
        panic!()
    };
    account.endpoint = Url::parse(&format!("http://{address}/auth/v1/token")).unwrap();
    let auth = Arc::new(auth);
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let auth = auth.clone();
        tasks.push(tokio::spawn(async move { auth.bearer().await.unwrap() }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap(), "access-0");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(auth.bearer().await.unwrap(), "access-1");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    task.abort();
}
#[tokio::test]
async fn failures_are_redacted_and_back_off() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/auth/v1/token",
        post(move || {
            let seen = seen.clone();
            async move {
                seen.fetch_add(1, Ordering::SeqCst);
                (StatusCode::SERVICE_UNAVAILABLE, "provider secret error")
            }
        }),
    );
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut auth = Credentials::password(
        "https://example.com",
        "sb_publishable_test".into(),
        "worker@example.com".into(),
        "technical-secret".into(),
    )
    .unwrap();
    let Mode::Technical(account) = &mut auth.mode else {
        panic!()
    };
    account.endpoint = Url::parse(&format!("http://{address}/auth/v1/token")).unwrap();
    for _ in 0..8 {
        let error = auth.bearer().await.unwrap_err();
        assert_eq!(error, Error::Unavailable);
        assert!(!error.to_string().contains("secret"));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    task.abort();
}
#[test]
fn unsafe_origins_keys_and_headers_reject() {
    for origin in [
        "http://example.com",
        "https://user@example.com",
        "https://example.com/path",
        "https://example.com/?q=x",
    ] {
        assert!(
            Credentials::password(
                origin,
                "sb_publishable_test".into(),
                "a@b".into(),
                "secret".into()
            )
            .is_err()
        );
    }
    for key in ["sb_secret_not_allowed", "service-role-jwt", ""] {
        assert!(
            Credentials::password(
                "https://example.com",
                key.into(),
                "a@b".into(),
                "secret".into()
            )
            .is_err()
        );
    }
    assert!(Credentials::fixed("public".into(), "bad\r\nheader".into()).is_err());
}

#[tokio::test]
async fn revoked_refresh_reauthenticates_without_persisting_rotated_secrets() {
    use axum::response::IntoResponse;
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app=Router::new().route("/auth/v1/token",post(move|Query(query):Query<HashMap<String,String>>|{let seen=seen.clone();async move{let n=seen.fetch_add(1,Ordering::SeqCst);if n==1{assert_eq!(query["grant_type"],"refresh_token");return (StatusCode::BAD_REQUEST,"do not expose this error").into_response()};assert_eq!(query["grant_type"],"password");Json(json!({"access_token":format!("token-{n}"),"refresh_token":"rotated-secret","expires_in":1})).into_response()}}));
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut auth = Credentials::password(
        "https://example.com",
        "sb_publishable_test".into(),
        "worker@example.com".into(),
        "technical-secret".into(),
    )
    .unwrap();
    let Mode::Technical(account) = &mut auth.mode else {
        panic!()
    };
    account.endpoint = Url::parse(&format!("http://{address}/auth/v1/token")).unwrap();
    assert_eq!(auth.bearer().await.unwrap(), "token-0");
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(auth.bearer().await.unwrap(), "token-2");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    task.abort();
}

#[tokio::test]
async fn oversized_or_redirected_auth_responses_fail_closed() {
    use axum::{response::IntoResponse, routing::any};
    let redirected = Arc::new(AtomicUsize::new(0));
    let seen = redirected.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route(
            "/auth/v1/token",
            post(move |Json(body): Json<Value>| async move {
                if body["email"] == "large@example.com" {
                    return "x".repeat(65537).into_response();
                };
                (StatusCode::TEMPORARY_REDIRECT, [("location", "/forbidden")]).into_response()
            }),
        )
        .route(
            "/forbidden",
            any(move || {
                let seen = seen.clone();
                async move {
                    seen.fetch_add(1, Ordering::SeqCst);
                    StatusCode::OK
                }
            }),
        );
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    for email in ["large@example.com", "redirect@example.com"] {
        let mut auth = Credentials::password(
            "https://example.com",
            "sb_publishable_test".into(),
            email.into(),
            "technical-secret".into(),
        )
        .unwrap();
        let Mode::Technical(account) = &mut auth.mode else {
            panic!()
        };
        account.endpoint = Url::parse(&format!("http://{address}/auth/v1/token")).unwrap();
        assert_eq!(auth.bearer().await.unwrap_err(), Error::Unavailable);
    }
    assert_eq!(redirected.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn wall_clock_expiry_and_storage_401_force_refresh_without_replaying_writes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let requests = Arc::new(AtomicUsize::new(0));
    let counted = requests.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app=Router::new().route("/auth/v1/token",post(move||{let seen=seen.clone();async move{let n=seen.fetch_add(1,Ordering::SeqCst);Json(json!({"access_token":format!("access-{n}"),"refresh_token":format!("refresh-{n}"),"expires_in":3600}))}})).route("/object",post(move|headers:HeaderMap|{let counted=counted.clone();async move{assert_eq!(headers["apikey"],"sb_publishable_test");counted.fetch_add(1,Ordering::SeqCst);StatusCode::UNAUTHORIZED}}));
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut auth = Credentials::password(
        "https://example.com",
        "sb_publishable_test".into(),
        "worker@example.com".into(),
        "technical-secret".into(),
    )
    .unwrap();
    let Mode::Technical(account) = &mut auth.mode else {
        panic!()
    };
    account.endpoint = Url::parse(&format!("http://{address}/auth/v1/token")).unwrap();
    let wrong = auth
        .send(Client::new().post("https://unrelated.invalid/object"))
        .await;
    assert_eq!(wrong.err().unwrap(), Error::InvalidConfiguration);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(auth.bearer().await.unwrap(), "access-0");
    let Mode::Technical(account) = &auth.mode else {
        panic!()
    };
    account
        .state
        .lock()
        .await
        .session
        .as_mut()
        .unwrap()
        .wall_valid_until = SystemTime::UNIX_EPOCH;
    assert_eq!(auth.bearer().await.unwrap(), "access-1");
    let response = auth
        .send(Client::new().post(format!("http://{address}/object")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    assert_eq!(auth.bearer().await.unwrap(), "access-2");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    task.abort();
}
