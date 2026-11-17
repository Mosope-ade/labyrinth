//! Ephemeral asymmetric test issuer. Tokens still pass the production verifier.
use super::{AuthConfig, JwksVerifier};
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use p256::{SecretKey, elliptic_curve::Generate, pkcs8::EncodePrivateKey};
use serde_json::{Value, json};
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;
pub(crate) const ISSUER: &str = "https://test-project.supabase.co/auth/v1";
pub(crate) struct Signer {
    pub kid: String,
    key: EncodingKey,
    pub jwk: Value,
}
impl Signer {
    pub fn new(kid: &str) -> Self {
        let secret = SecretKey::generate_from_rng(&mut rand::rng());
        let point = secret.public_key().to_sec1_bytes();
        let jwk = json!({"kty":"EC","crv":"P-256","alg":"ES256","use":"sig","key_ops":["verify"],"kid":kid,"x":URL_SAFE_NO_PAD.encode(&point[1..33]),"y":URL_SAFE_NO_PAD.encode(&point[33..65])});
        Self {
            kid: kid.into(),
            key: EncodingKey::from_ec_der(secret.to_pkcs8_der().unwrap().as_bytes()),
            jwk,
        }
    }
    pub fn token(&self, claims: &Value) -> String {
        let mut h = Header::new(Algorithm::ES256);
        h.kid = Some(self.kid.clone());
        self.token_with_header(claims, &h)
    }
    pub fn token_with_header(&self, claims: &Value, header: &Header) -> String {
        jsonwebtoken::encode(header, claims, &self.key).unwrap()
    }
}
pub(crate) fn claims(subject: Uuid) -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    json!({"iss":ISSUER,"aud":"authenticated","sub":subject,"role":"authenticated","is_anonymous":false,"session_id":Uuid::new_v4(),"iat":now-1,"exp":now+600})
}
#[derive(Clone)]
struct Source {
    body: Arc<RwLock<Value>>,
    status: Arc<RwLock<StatusCode>>,
    hits: Arc<AtomicUsize>,
}
pub(crate) struct TestIssuer {
    pub signer: Signer,
    source: Source,
    url: reqwest::Url,
    task: tokio::task::JoinHandle<()>,
}
impl TestIssuer {
    pub async fn start() -> Self {
        let signer = Signer::new("key-one");
        let source = Source {
            body: Arc::new(RwLock::new(json!({"keys":[signer.jwk.clone()]}))),
            status: Arc::new(RwLock::new(StatusCode::OK)),
            hits: Arc::new(AtomicUsize::new(0)),
        };
        let app = Router::new()
            .route(
                "/jwks",
                get(|State(s): State<Source>| async move {
                    s.hits.fetch_add(1, Ordering::SeqCst);
                    (
                        *s.status.read().unwrap(),
                        Json(s.body.read().unwrap().clone()),
                    )
                }),
            )
            .with_state(source.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/jwks", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            signer,
            source,
            url,
            task,
        }
    }
    pub async fn verifier(&self) -> Result<JwksVerifier, super::AuthError> {
        JwksVerifier::load(AuthConfig::loopback(ISSUER, self.url.clone())).await
    }
    pub fn replace_keys(&self, keys: Vec<Value>) {
        *self.source.body.write().unwrap() = json!({"keys":keys});
    }
    pub fn fail(&self) {
        *self.source.status.write().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    }
    pub fn hits(&self) -> usize {
        self.source.hits.load(Ordering::SeqCst)
    }
}
impl Drop for TestIssuer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
