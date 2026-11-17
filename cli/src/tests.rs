use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

#[test]
fn configuration_confines_credentials_and_local_api_exception() {
    assert!(
        Config::new(
            "http://127.0.0.1:3000",
            "https://test.supabase.co",
            "sb_publishable_test"
        )
        .is_ok()
    );
    for api in [
        "http://localhost:3000",
        "http://example.com",
        "https://user:pass@example.com",
        "https://example.com/path",
        "https://example.com?x=1",
    ] {
        assert!(Config::new(api, "https://test.supabase.co", "sb_publishable_test").is_err());
    }
    assert!(
        Config::new(
            "https://api.example.com",
            "http://127.0.0.1:9999",
            "sb_publishable_test"
        )
        .is_err()
    );
    for key in ["service_role", "sb_secret_bad", "sb_publishable_\r\nx: y"] {
        assert!(Config::new("https://api.example.com", "https://test.supabase.co", key).is_err());
    }
}

#[test]
fn pkce_uses_rfc7636_s256_and_independent_random_correlation() {
    let a = callback::Pkce::new().unwrap();
    let b = callback::Pkce::new().unwrap();
    assert_eq!(a.verifier.len(), 43);
    assert_eq!(
        a.challenge,
        URL_SAFE_NO_PAD.encode(Sha256::digest(a.verifier.as_bytes()))
    );
    assert_ne!(a.verifier, b.verifier);
    assert_ne!(a.state, b.state);
    assert_ne!(a.verifier, a.state);
}

#[tokio::test]
async fn callback_rejects_foreign_requests_and_consumes_only_one_valid_code() {
    let mut cb = Callback::bind(0).await.unwrap();
    let http = reqwest::Client::new();
    let url = cb.redirect_url().to_owned();
    let bad = http
        .get(&url)
        .query(&[("code", "code-one"), ("state", "wrong")])
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 400);
    let bad = http
        .get(&url)
        .header("host", "evil.example")
        .query(&[("code", "code-one")])
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 400);
    let bad = http
        .get(&url)
        .query(&[("code", "code-one"), ("code", "duplicate")])
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 400);
    let ok = http
        .get(&url)
        .query(&[("code", "code-one")])
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), 200);
    assert_eq!(ok.headers()["referrer-policy"], "no-referrer");
    assert_eq!(ok.headers()["cache-control"], "no-store");
    assert!(
        ok.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("default-src 'none'")
    );
    assert!(!ok.text().await.unwrap().contains("code-one"));
    assert_eq!(
        cb.receive(std::time::Duration::from_secs(1)).await.unwrap(),
        "code-one"
    );
    let replay = http
        .get(&url)
        .query(&[("code", "code-two")])
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), 400);
}

#[tokio::test]
async fn callback_rejects_foreign_duplicate_and_oversized_requests_without_consuming_login() {
    let mut callback = Callback::bind(0).await.unwrap();
    assert_eq!(callback.pkce.verifier.len(), 43);
    assert_eq!(
        callback.pkce.challenge,
        URL_SAFE_NO_PAD.encode(Sha256::digest(&callback.pkce.verifier))
    );
    assert_ne!(callback.pkce.state, callback.pkce.verifier);
    let http = reqwest::Client::new();
    let url = callback.redirect_url().to_owned();
    let foreign = url.replace(&callback.pkce.state, "wrong");
    for bad in [
        format!("{foreign}&code=abc"),
        format!("{url}&code=abc&code=def"),
        format!("{url}&code=abc&state=duplicate"),
        format!("{url}&code={}", "a".repeat(8193)),
        format!("{url}&code=%3Cscript%3E"),
    ] {
        assert_eq!(http.get(bad).send().await.unwrap().status(), 400);
    }
    assert_eq!(
        http.get(format!("{url}&code=abc"))
            .header("host", "evil.example")
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let response = http.get(format!("{url}&code=abc")).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    assert_eq!(response.headers()["cache-control"], "no-store");
    let html = response.text().await.unwrap();
    assert!(html.contains("history.replaceState"));
    assert!(!html.contains(&callback.pkce.state));
    assert_eq!(
        callback
            .receive(std::time::Duration::from_secs(1))
            .await
            .unwrap(),
        "abc"
    );
    assert!(matches!(
        Callback::bind(reqwest::Url::parse(&url).unwrap().port().unwrap()).await,
        Err(Error::CallbackBind)
    ));
}

#[tokio::test]
async fn callback_timeout_releases_port_when_dropped() {
    let mut callback = Callback::bind(0).await.unwrap();
    let port = reqwest::Url::parse(callback.redirect_url())
        .unwrap()
        .port()
        .unwrap();
    assert_eq!(
        callback.receive(std::time::Duration::from_millis(1)).await,
        Err(Error::LoginExpired)
    );
    drop(callback);
    tokio::task::yield_now().await;
    assert!(Callback::bind(port).await.is_ok());
}

#[derive(Default)]
pub(super) struct MemoryVault {
    value: std::sync::Mutex<Option<String>>,
    fail_save: bool,
}
impl Vault for MemoryVault {
    fn load(&self) -> Result<Option<String>, Error> {
        Ok(self.value.lock().unwrap().clone())
    }
    fn save(&self, value: &str) -> Result<(), Error> {
        if self.fail_save {
            return Err(Error::CredentialStore);
        }
        *self.value.lock().unwrap() = Some(value.to_owned());
        Ok(())
    }
    fn clear(&self) -> Result<(), Error> {
        *self.value.lock().unwrap() = None;
        Ok(())
    }
}
#[derive(Clone)]
pub(super) struct Provider {
    redirect: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    challenge: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    used: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(super) events: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    reply: std::sync::Arc<std::sync::Mutex<serde_json::Value>>,
    token_status: std::sync::Arc<std::sync::atomic::AtomicU16>,
    api_status: std::sync::Arc<std::sync::atomic::AtomicU16>,
}
fn session() -> serde_json::Value {
    serde_json::json!({"access_token":"access-secret","refresh_token":"rotated-secret","token_type":"bearer","expires_in":3600,"user":{"id":"11111111-1111-4111-8111-111111111111","is_anonymous":false,"email_confirmed_at":"2026-11-17T00:00:00Z"}})
}
pub(super) async fn provider() -> (Client, Provider, tokio::task::JoinHandle<()>) {
    use axum::{
        Json, Router,
        extract::{Query, State},
        http::{HeaderMap, StatusCode},
        routing::{get, post},
    };
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU16, Ordering},
    };
    let p = Provider {
        redirect: Arc::new(Mutex::new(None)),
        challenge: Arc::new(Mutex::new(None)),
        used: Arc::new(AtomicBool::new(false)),
        events: Arc::new(Mutex::new(vec![])),
        reply: Arc::new(Mutex::new(session())),
        token_status: Arc::new(AtomicU16::new(200)),
        api_status: Arc::new(AtomicU16::new(200)),
    };
    async fn otp(
        State(p): State<Provider>,
        headers: HeaderMap,
        Query(q): Query<std::collections::HashMap<String, String>>,
        Json(body): Json<serde_json::Value>,
    ) -> StatusCode {
        assert_eq!(headers["apikey"], "sb_publishable_test");
        assert!(headers.get("authorization").is_none());
        assert_eq!(body["code_challenge_method"], "s256");
        assert_eq!(body["create_user"], true);
        assert!(q["redirect_to"].starts_with("http://127.0.0.1:"));
        *p.redirect.lock().unwrap() = Some(q["redirect_to"].clone());
        *p.challenge.lock().unwrap() = Some(body["code_challenge"].as_str().unwrap().to_owned());
        p.events.lock().unwrap().push("otp".to_string());
        StatusCode::OK
    }
    async fn exchange(
        State(p): State<Provider>,
        Query(q): Query<std::collections::HashMap<String, String>>,
        Json(body): Json<serde_json::Value>,
    ) -> (StatusCode, Json<serde_json::Value>) {
        let grant = &q["grant_type"];
        p.events.lock().unwrap().push(grant.clone());
        if grant == "pkce" {
            let valid = body["auth_code"] == "valid-code"
                && body["code_verifier"].as_str().is_some_and(|v| {
                    Some(URL_SAFE_NO_PAD.encode(Sha256::digest(v))) == *p.challenge.lock().unwrap()
                });
            if !valid || p.used.swap(true, Ordering::SeqCst) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"message":"secret-provider-error"})),
                );
            }
        } else {
            assert_eq!(grant, "refresh_token");
            assert!(body["refresh_token"].as_str().is_some());
        }
        (
            StatusCode::from_u16(p.token_status.load(Ordering::SeqCst)).unwrap(),
            Json(p.reply.lock().unwrap().clone()),
        )
    }
    async fn logout(
        State(p): State<Provider>,
        headers: HeaderMap,
        Query(q): Query<std::collections::HashMap<String, String>>,
    ) -> StatusCode {
        assert_eq!(q["scope"], "local");
        assert_eq!(headers["authorization"], "Bearer access-secret");
        p.events.lock().unwrap().push("logout".into());
        StatusCode::NO_CONTENT
    }
    async fn api(
        State(p): State<Provider>,
        headers: HeaderMap,
    ) -> (StatusCode, Json<serde_json::Value>) {
        assert_eq!(headers["authorization"], "Bearer access-secret");
        assert!(headers.get("apikey").is_none());
        p.events.lock().unwrap().push("api".into());
        (
            StatusCode::from_u16(p.api_status.load(Ordering::SeqCst)).unwrap(),
            Json(serde_json::json!({"user_id":"test","provisioned":false})),
        )
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/auth/v1/otp", post(otp))
        .route("/auth/v1/token", post(exchange))
        .route("/auth/v1/logout", post(logout))
        .route("/v1/me", get(api).post(api))
        .with_state(p.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let mut config = Config::new(&url, "https://test.supabase.co", "sb_publishable_test").unwrap();
    config.auth = reqwest::Url::parse(&url).unwrap();
    (Client::new(config).unwrap(), p, task)
}

#[tokio::test]
async fn login_exchange_refresh_api_and_local_logout_preserve_boundaries() {
    let (client, p, task) = provider().await;
    let vault = MemoryVault::default();
    let callback = Callback::bind(0).await.unwrap();
    client.begin("user@example.com", &callback).await.unwrap();
    assert_eq!(
        client.finish("valid-code", "wrong-verifier", &vault).await,
        Err(Error::Unauthenticated)
    );
    assert!(vault.load().unwrap().is_none());
    client
        .finish("valid-code", &callback.pkce.verifier, &vault)
        .await
        .unwrap();
    let saved = vault.load().unwrap().unwrap();
    assert!(saved.contains("rotated-secret"));
    assert!(!saved.contains("access-secret"));
    assert!(!saved.contains("user@example.com"));
    assert_eq!(
        client
            .finish("valid-code", &callback.pkce.verifier, &vault)
            .await,
        Err(Error::Unauthenticated)
    );
    assert_eq!(
        client.login("user@example.com", 0, &vault).await,
        Err(Error::AlreadyLoggedIn)
    );
    client
        .api(&vault, reqwest::Method::GET, "v1/me", None, None)
        .await
        .unwrap();
    client.logout(&vault).await.unwrap();
    assert!(vault.load().unwrap().is_none());
    assert_eq!(
        p.events
            .lock()
            .unwrap()
            .iter()
            .filter(|x| x.as_str() == "api")
            .count(),
        1
    );
    assert!(p.events.lock().unwrap().contains(&"logout".into()));
    task.abort();
}

pub(super) async fn signed_in(client: &Client, vault: &dyn Vault) {
    let callback = Callback::bind(0).await.unwrap();
    client.begin("user@example.com", &callback).await.unwrap();
    client
        .finish("valid-code", &callback.pkce.verifier, vault)
        .await
        .unwrap();
}
#[tokio::test]
async fn rejected_refresh_erases_credential_and_never_calls_api() {
    let (client, p, task) = provider().await;
    let vault = MemoryVault::default();
    signed_in(&client, &vault).await;
    p.token_status
        .store(400, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        client
            .api(&vault, reqwest::Method::GET, "v1/me", None, None)
            .await,
        Err(Error::Unauthenticated)
    );
    assert!(vault.load().unwrap().is_none());
    assert!(!p.events.lock().unwrap().contains(&"api".into()));
    task.abort();
}
#[tokio::test]
async fn refresh_outage_preserves_saved_login_and_logout_clears_it_with_warning() {
    let (client, p, task) = provider().await;
    let vault = MemoryVault::default();
    signed_in(&client, &vault).await;
    let saved = vault.load().unwrap();
    p.token_status
        .store(503, std::sync::atomic::Ordering::SeqCst);
    let error = client
        .api(&vault, reqwest::Method::GET, "v1/me", None, None)
        .await
        .unwrap_err();
    assert_eq!(error, Error::Unavailable);
    assert!(!error.to_string().contains("secret"));
    assert_eq!(saved, vault.load().unwrap());
    assert_eq!(client.logout(&vault).await, Err(Error::LogoutIncomplete));
    assert!(vault.load().unwrap().is_none());
    task.abort();
}
#[tokio::test]
async fn failed_rotation_save_revokes_session_before_any_api_call() {
    let (client, p, task) = provider().await;
    let good = MemoryVault::default();
    signed_in(&client, &good).await;
    let failing = MemoryVault {
        value: good.value,
        fail_save: true,
    };
    assert_eq!(
        client
            .api(
                &failing,
                reqwest::Method::POST,
                "v1/me",
                Some(serde_json::json!({})),
                None
            )
            .await,
        Err(Error::CredentialStore)
    );
    assert!(failing.load().unwrap().is_none());
    let events = p.events.lock().unwrap();
    assert!(events.contains(&"logout".into()));
    assert!(!events.contains(&"api".into()));
    task.abort();
}
#[tokio::test]
async fn changed_subject_cannot_replace_identity() {
    let (client, p, task) = provider().await;
    let vault = MemoryVault::default();
    signed_in(&client, &vault).await;
    p.reply.lock().unwrap()["user"]["id"] =
        serde_json::json!("22222222-2222-4222-8222-222222222222");
    assert_eq!(
        client
            .api(&vault, reqwest::Method::GET, "v1/me", None, None)
            .await,
        Err(Error::InvalidResponse)
    );
    assert!(vault.load().unwrap().is_none());
    task.abort();
}
#[tokio::test]
async fn api_errors_and_redirects_never_replay_mutations_or_expose_provider_body() {
    let (client, p, task) = provider().await;
    let vault = MemoryVault::default();
    signed_in(&client, &vault).await;
    for status in [302, 401, 403, 409, 429, 500] {
        p.api_status
            .store(status, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            client
                .api(
                    &vault,
                    reqwest::Method::POST,
                    "v1/me",
                    Some(serde_json::json!({})),
                    Some("same-intent-key")
                )
                .await,
            Err(Error::Api(status))
        );
    }
    assert_eq!(
        p.events
            .lock()
            .unwrap()
            .iter()
            .filter(|x| x.as_str() == "api")
            .count(),
        6
    );
    task.abort();
}
#[tokio::test]
async fn invalid_paths_keys_and_provider_payloads_fail_closed() {
    let (client, p, task) = provider().await;
    let vault = MemoryVault::default();
    for path in [
        "https://evil.example/v1/me",
        "//evil.example/v1/me",
        "v1/../me",
        "v1/%2e%2e/me",
        "v1/me?x=1",
    ] {
        assert_eq!(
            client
                .api(&vault, reqwest::Method::GET, path, None, None)
                .await,
            Err(Error::InvalidInput)
        );
    }
    assert_eq!(
        client
            .api(
                &vault,
                reqwest::Method::POST,
                "v1/me",
                None,
                Some("bad\r\nsecret")
            )
            .await,
        Err(Error::InvalidInput)
    );
    signed_in(&client, &vault).await;
    for bad in [
        serde_json::json!({}),
        serde_json::json!({"access_token":"x".repeat(65537)}),
        serde_json::json!({"access_token":"secret\nheader"}),
    ] {
        *p.reply.lock().unwrap() = bad;
        assert_eq!(
            client
                .api(&vault, reqwest::Method::GET, "v1/me", None, None)
                .await,
            Err(Error::InvalidResponse)
        );
    }
    assert!(!p.events.lock().unwrap().contains(&"api".into()));
    task.abort();
}

#[test]
fn advisory_profile_lock_excludes_other_process_handles_and_releases_after_drop() {
    let dir = std::env::temp_dir().join(format!(
        "laby-lock-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("profile.lock");
    let a = store::lock(&path).unwrap();
    assert!(matches!(store::lock(&path), Err(Error::Busy)));
    drop(a);
    drop(store::lock(&path).unwrap());
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn anonymous_unconfirmed_and_bad_token_fields_never_reach_api() {
    let (client, p, task) = provider().await;
    let vault = MemoryVault::default();
    signed_in(&client, &vault).await;
    for (field, value) in [
        ("is_anonymous", serde_json::json!(true)),
        ("email_confirmed_at", serde_json::Value::Null),
        ("id", serde_json::json!(uuid::Uuid::nil())),
    ] {
        let mut reply = session();
        reply["user"][field] = value;
        *p.reply.lock().unwrap() = reply;
        assert_eq!(
            client
                .api(&vault, reqwest::Method::GET, "v1/me", None, None)
                .await,
            Err(Error::InvalidResponse)
        );
    }
    for (field, value) in [
        ("token_type", serde_json::json!("other")),
        ("expires_in", serde_json::json!(0)),
        ("access_token", serde_json::json!("secret\nheader")),
        ("refresh_token", serde_json::json!("")),
    ] {
        let mut reply = session();
        reply[field] = value;
        *p.reply.lock().unwrap() = reply;
        assert_eq!(
            client
                .api(&vault, reqwest::Method::GET, "v1/me", None, None)
                .await,
            Err(Error::InvalidResponse)
        );
    }
    assert!(!p.events.lock().unwrap().contains(&"api".into()));
    task.abort();
}

#[tokio::test]
async fn login_waits_for_real_loopback_callback_then_saves_exchange() {
    let (client, p, task) = provider().await;
    let vault = MemoryVault::default();
    let callback = async {
        let url = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let redirect = p.redirect.lock().unwrap().clone();
                if let Some(url) = redirect {
                    break url;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let response = reqwest::get(format!("{url}&code=valid-code"))
            .await
            .unwrap();
        assert!(response.status().is_success());
    };
    let (result, ()) = tokio::join!(client.login("user@example.com", 0, &vault), callback);
    result.unwrap();
    assert!(vault.load().unwrap().is_some());
    task.abort();
}
