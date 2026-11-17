use crate::{Callback, Config, Error, Vault, store::Saved};
use reqwest::{Method, StatusCode, header::HeaderValue};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;

pub struct Client {
    pub(crate) config: Config,
    http: reqwest::Client,
}
#[derive(Deserialize)]
struct User {
    id: Uuid,
    is_anonymous: bool,
    email_confirmed_at: Option<String>,
}
#[derive(Deserialize)]
struct Session {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_in: u64,
    user: User,
}
fn token(value: &str) -> bool {
    !value.is_empty() && value.len() <= 16384 && HeaderValue::from_str(value).is_ok()
}
impl Client {
    pub fn new(config: Config) -> Result<Self, Error> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| Error::Unavailable)?;
        Ok(Self { config, http })
    }
    fn auth_request(&self, method: Method, path: &str) -> Result<reqwest::RequestBuilder, Error> {
        let url = self
            .config
            .auth
            .join(path)
            .map_err(|_| Error::InvalidInput)?;
        let mut key = HeaderValue::from_str(&self.config.key).map_err(|_| Error::InvalidInput)?;
        key.set_sensitive(true);
        Ok(self
            .http
            .request(method, url)
            .header("apikey", key)
            .header("x-supabase-api-version", "2024-01-01"))
    }
    pub async fn login(&self, email: &str, port: u16, vault: &dyn Vault) -> Result<(), Error> {
        if vault.load()?.is_some() {
            return Err(Error::AlreadyLoggedIn);
        }
        let mut callback = Callback::bind(port).await?;
        self.begin(email, &callback).await?;
        let code = tokio::select! {
            reply=callback.receive(Duration::from_secs(300))=>reply?,
            _=tokio::signal::ctrl_c()=>return Err(Error::LoginExpired),
        };
        self.finish(&code, &callback.pkce.verifier, vault).await
    }
    pub(crate) async fn begin(&self, email: &str, callback: &Callback) -> Result<(), Error> {
        if email.trim() != email
            || email.is_empty()
            || email.len() > 320
            || !email.contains('@')
            || email.chars().any(char::is_control)
        {
            return Err(Error::InvalidInput);
        }
        let response=self.auth_request(Method::POST,"auth/v1/otp")?.query(&[("redirect_to",callback.redirect_url())]).json(&json!({"email":email,"create_user":true,"code_challenge":callback.pkce.challenge,"code_challenge_method":"s256"})).send().await.map_err(|_|Error::Unavailable)?;
        if !response.status().is_success() {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub(crate) async fn finish(
        &self,
        code: &str,
        verifier: &str,
        vault: &dyn Vault,
    ) -> Result<(), Error> {
        let session = self
            .exchange("pkce", json!({"auth_code":code,"code_verifier":verifier}))
            .await?;
        self.save(&session, vault).await
    }
    async fn exchange(&self, grant: &str, body: Value) -> Result<Session, Error> {
        let response = self
            .auth_request(Method::POST, "auth/v1/token")?
            .query(&[("grant_type", grant)])
            .json(&body)
            .send()
            .await
            .map_err(|_| Error::Unavailable)?;
        if [
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::UNPROCESSABLE_ENTITY,
        ]
        .contains(&response.status())
        {
            return Err(Error::Unauthenticated);
        }
        if !response.status().is_success() {
            return Err(Error::Unavailable);
        }
        let bytes = bounded(response, 65536).await?;
        let session: Session =
            serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
        if !token(&session.access_token)
            || !token(&session.refresh_token)
            || !session.token_type.eq_ignore_ascii_case("bearer")
            || !(1..=86400).contains(&session.expires_in)
            || session.user.id.is_nil()
            || session.user.is_anonymous
            || session
                .user
                .email_confirmed_at
                .as_ref()
                .is_none_or(|v| v.is_empty())
        {
            return Err(Error::InvalidResponse);
        }
        Ok(session)
    }
    async fn save(&self, session: &Session, vault: &dyn Vault) -> Result<(), Error> {
        let saved = Saved {
            version: 1,
            refresh_token: session.refresh_token.clone(),
            subject: session.user.id,
        };
        let value = serde_json::to_string(&saved).map_err(|_| Error::InvalidResponse)?;
        if vault.save(&value).is_err() {
            let _ = self.revoke(&session.access_token).await;
            let _ = vault.clear();
            return Err(Error::CredentialStore);
        }
        Ok(())
    }
    async fn refresh(&self, vault: &dyn Vault) -> Result<Session, Error> {
        let raw = vault.load()?.ok_or(Error::NotLoggedIn)?;
        if raw.len() > 32768 {
            return Err(Error::CredentialStore);
        }
        let saved: Saved = serde_json::from_str(&raw).map_err(|_| Error::CredentialStore)?;
        if saved.version != 1 || !token(&saved.refresh_token) || saved.subject.is_nil() {
            return Err(Error::CredentialStore);
        }
        let session = match self
            .exchange(
                "refresh_token",
                json!({"refresh_token":saved.refresh_token}),
            )
            .await
        {
            Err(Error::Unauthenticated) => {
                vault.clear()?;
                return Err(Error::Unauthenticated);
            }
            result => result?,
        };
        if session.user.id != saved.subject {
            let _ = self.revoke(&session.access_token).await;
            vault.clear()?;
            return Err(Error::InvalidResponse);
        }
        self.save(&session, vault).await?;
        Ok(session)
    }
    async fn revoke(&self, bearer: &str) -> Result<(), Error> {
        let response = self
            .auth_request(Method::POST, "auth/v1/logout")?
            .query(&[("scope", "local")])
            .bearer_auth(bearer)
            .send()
            .await
            .map_err(|_| Error::Unavailable)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Error::Unavailable)
        }
    }
    pub async fn logout(&self, vault: &dyn Vault) -> Result<(), Error> {
        if vault.load()?.is_none() {
            return Ok(());
        }
        let remote = match self.refresh(vault).await {
            Ok(session) => self.revoke(&session.access_token).await,
            Err(error) => Err(error),
        };
        vault.clear()?;
        remote.map_err(|_| Error::LogoutIncomplete)
    }
    /// Typed command paths are assembled by the binary; this boundary rejects cross-origin URLs.
    pub async fn api(
        &self,
        vault: &dyn Vault,
        method: Method,
        path: &str,
        body: Option<Value>,
        idempotency: Option<&str>,
    ) -> Result<Value, Error> {
        if !path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_".contains(&b))
            || !path.starts_with("v1/")
            || path.contains(['?', '#', '\\'])
            || path.split('/').any(|p| p == ".." || p == ".")
        {
            return Err(Error::InvalidInput);
        }
        if idempotency.is_some_and(|v| {
            v.is_empty()
                || v.len() > 128
                || !v
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
        }) {
            return Err(Error::InvalidInput);
        }
        let url = self
            .config
            .api
            .join(path)
            .map_err(|_| Error::InvalidInput)?;
        if url.origin() != self.config.api.origin() {
            return Err(Error::InvalidInput);
        }
        let session = self.refresh(vault).await?;
        let mut request = self
            .http
            .request(method, url)
            .bearer_auth(&session.access_token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        if let Some(key) = idempotency {
            request = request.header("idempotency-key", key);
        }
        let response = request.send().await.map_err(|_| Error::Unavailable)?;
        if !response.status().is_success() {
            return Err(Error::Api(response.status().as_u16()));
        }
        let bytes = bounded(response, 2 * 1024 * 1024).await?;
        serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)
    }
}
async fn bounded(mut response: reqwest::Response, max: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Unavailable)? {
        if bytes.len() + chunk.len() > max {
            return Err(Error::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
