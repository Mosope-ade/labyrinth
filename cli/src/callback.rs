use crate::Error;
use axum::{
    Router,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::{
    net::TcpListener,
    sync::{Mutex, oneshot},
    task::JoinHandle,
};

pub(crate) struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}
fn random() -> Result<String, Error> {
    let mut bytes = [0; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| Error::Unavailable)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
impl Pkce {
    pub fn new() -> Result<Self, Error> {
        let verifier = random()?;
        Ok(Self {
            challenge: URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
            verifier,
            state: random()?,
        })
    }
}
struct Pending {
    host: String,
    state: String,
    sender: Mutex<Option<oneshot::Sender<String>>>,
}
pub struct Callback {
    pub(crate) pkce: Pkce,
    redirect: String,
    receiver: Option<oneshot::Receiver<String>>,
    task: JoinHandle<()>,
}
impl Callback {
    pub async fn bind(port: u16) -> Result<Self, Error> {
        let pkce = Pkce::new()?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|_| Error::CallbackBind)?;
        let host = listener
            .local_addr()
            .map_err(|_| Error::CallbackBind)?
            .to_string();
        let (sender, receiver) = oneshot::channel();
        let redirect = format!("http://{host}/callback?state={}", pkce.state);
        let pending = Arc::new(Pending {
            host,
            state: pkce.state.clone(),
            sender: Mutex::new(Some(sender)),
        });
        let app = Router::new()
            .route("/callback", get(receive))
            .with_state(pending);
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self {
            pkce,
            redirect,
            receiver: Some(receiver),
            task,
        })
    }
    pub fn redirect_url(&self) -> &str {
        &self.redirect
    }
    pub async fn receive(&mut self, timeout: Duration) -> Result<String, Error> {
        let receiver = self.receiver.take().ok_or(Error::LoginExpired)?;
        tokio::time::timeout(timeout, receiver)
            .await
            .map_err(|_| Error::LoginExpired)?
            .map_err(|_| Error::LoginExpired)
    }
}
impl Drop for Callback {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn receive(
    State(p): State<Arc<Pending>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let code = (|| {
        if headers.get("host").and_then(|v| v.to_str().ok()) != Some(p.host.as_str()) {
            return None;
        }
        let query = query.as_deref()?;
        if query.len() > 8192 {
            return None;
        }
        let pairs = reqwest::Url::parse(&format!("http://127.0.0.1/?{query}")).ok()?;
        let mut state = None;
        let mut code = None;
        for (key, value) in pairs.query_pairs() {
            match key.as_ref() {
                "state" if state.is_none() => state = Some(value.into_owned()),
                "code" if code.is_none() => code = Some(value.into_owned()),
                _ => return None,
            }
        }
        if state.as_deref() != Some(p.state.as_str()) {
            return None;
        }
        let code = code?;
        if code.is_empty()
            || code.len() > 2048
            || !code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return None;
        }
        Some(code)
    })();
    let accepted = if let Some(code) = code {
        p.sender
            .lock()
            .await
            .take()
            .is_some_and(|sender| sender.send(code).is_ok())
    } else {
        false
    };
    let script = "history.replaceState(null,'','/complete');";
    let hash = STANDARD.encode(Sha256::digest(script));
    let message = if accepted {
        "Callback received. Return to your terminal to check login."
    } else {
        "Invalid or expired callback. Return to your terminal."
    };
    let mut response=(if accepted {StatusCode::OK} else {StatusCode::BAD_REQUEST},Html(format!("<!doctype html><meta charset=utf-8><title>Labyrinth login</title><script>{script}</script><p>{message}</p>"))).into_response();
    for (key, value) in [
        ("cache-control", "no-store".to_string()),
        ("referrer-policy", "no-referrer".to_string()),
        ("x-content-type-options", "nosniff".to_string()),
        (
            "content-security-policy",
            format!(
                "default-src 'none'; script-src 'sha256-{hash}'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'"
            ),
        ),
    ] {
        response.headers_mut().insert(
            axum::http::HeaderName::from_static(key),
            value.parse().expect("constant header"),
        );
    }
    response
}
