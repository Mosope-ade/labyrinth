//! Server-only technical identity sessions. Never serialize/debug credentials.
use reqwest::{Client, Url, header::HeaderValue};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::Mutex;
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfiguration,
    Unavailable,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Storage identity authentication failed")
    }
}
impl std::error::Error for Error {}
struct Session {
    access: String,
    refresh: String,
    valid_until: Instant,
    wall_valid_until: SystemTime,
}
#[derive(Default)]
struct State {
    session: Option<Session>,
    retry_after: Option<Instant>,
}
struct Technical {
    endpoint: Url,
    client: Client,
    email: String,
    password: String,
    state: Mutex<State>,
}
enum Mode {
    Fixed(String),
    Technical(Box<Technical>),
}
pub struct Credentials {
    api_key: String,
    mode: Mode,
}
fn header(value: &str) -> bool {
    !value.is_empty() && value.len() <= 16384 && HeaderValue::from_str(value).is_ok()
}
impl Credentials {
    /// Low-level transport injection, including deterministic offline HTTP tests.
    pub fn fixed(api_key: String, token: String) -> Result<Self, Error> {
        if !header(&api_key) || !header(&token) {
            return Err(Error::InvalidConfiguration);
        }
        Ok(Self {
            api_key,
            mode: Mode::Fixed(token),
        })
    }
    /// Runtime path: publishable gateway key plus a dedicated technical Auth identity.
    pub fn password(
        origin: &str,
        api_key: String,
        email: String,
        password: String,
    ) -> Result<Self, Error> {
        let origin = Url::parse(origin).map_err(|_| Error::InvalidConfiguration)?;
        if origin.scheme() != "https"
            || origin.host_str().is_none()
            || origin.path() != "/"
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
            || !api_key.starts_with("sb_publishable_")
            || !header(&api_key)
            || email.trim() != email
            || email.is_empty()
            || email.len() > 320
            || password.is_empty()
            || password.len() > 4096
        {
            return Err(Error::InvalidConfiguration);
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(2)
            .build()
            .map_err(|_| Error::Unavailable)?;
        Ok(Self {
            api_key,
            mode: Mode::Technical(Box::new(Technical {
                endpoint: origin
                    .join("auth/v1/token")
                    .map_err(|_| Error::InvalidConfiguration)?,
                client,
                email,
                password,
                state: Mutex::new(State::default()),
            })),
        })
    }
    /// Send only to the configured project origin; never retry a storage mutation.
    pub async fn send(&self, builder: reqwest::RequestBuilder) -> Result<reqwest::Response, Error> {
        let (client, request) = builder.build_split();
        let mut request = request.map_err(|_| Error::InvalidConfiguration)?;
        if let Mode::Technical(account) = &self.mode
            && request.url().origin() != account.endpoint.origin()
        {
            return Err(Error::InvalidConfiguration);
        }
        let token = self.bearer().await?;
        let mut bearer =
            HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| Error::Unavailable)?;
        bearer.set_sensitive(true);
        let mut key =
            HeaderValue::from_str(&self.api_key).map_err(|_| Error::InvalidConfiguration)?;
        key.set_sensitive(true);
        request
            .headers_mut()
            .insert(reqwest::header::AUTHORIZATION, bearer);
        request.headers_mut().insert("apikey", key);
        let response = client
            .execute(request)
            .await
            .map_err(|_| Error::Unavailable)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            && let Mode::Technical(account) = &self.mode
        {
            let mut state = account.state.lock().await;
            if let Some(session) = state.session.as_mut()
                && session.access == token
            {
                session.valid_until = Instant::now();
                session.wall_valid_until = SystemTime::UNIX_EPOCH;
            }
        }
        Ok(response)
    }
    async fn bearer(&self) -> Result<String, Error> {
        let Mode::Technical(account) = &self.mode else {
            let Mode::Fixed(token) = &self.mode else {
                unreachable!()
            };
            return Ok(token.clone());
        };
        let mut state = account.state.lock().await;
        if let Some(session) = &state.session
            && session.valid_until > Instant::now()
            && session.wall_valid_until > SystemTime::now()
        {
            return Ok(session.access.clone());
        }
        if state.retry_after.is_some_and(|time| time > Instant::now()) {
            return Err(Error::Unavailable);
        }
        let response = if let Some(session) = &state.session {
            match account
                .exchange(
                    &self.api_key,
                    "refresh_token",
                    json!({"refresh_token":session.refresh}),
                )
                .await
            {
                Ok(reply) => Ok(reply),
                Err(ExchangeError::Rejected) => account.login(&self.api_key).await,
                Err(error) => Err(error),
            }
        } else {
            account.login(&self.api_key).await
        };
        match response {
            Ok(reply) => {
                let token = reply.access_token.clone();
                let margin = (reply.expires_in / 5).min(60);
                state.session = Some(Session {
                    access: reply.access_token,
                    refresh: reply.refresh_token,
                    wall_valid_until: SystemTime::now()
                        + Duration::from_secs(reply.expires_in - margin),
                    valid_until: Instant::now() + Duration::from_secs(reply.expires_in - margin),
                });
                state.retry_after = None;
                Ok(token)
            }
            Err(_) => {
                state.retry_after = Some(Instant::now() + Duration::from_secs(5));
                Err(Error::Unavailable)
            }
        }
    }
}
#[derive(Deserialize)]
struct Reply {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}
enum ExchangeError {
    Rejected,
    Unavailable,
}
impl Technical {
    async fn login(&self, key: &str) -> Result<Reply, ExchangeError> {
        self.exchange(
            key,
            "password",
            json!({"email":self.email,"password":self.password}),
        )
        .await
    }
    async fn exchange(&self, key: &str, grant: &str, body: Value) -> Result<Reply, ExchangeError> {
        let mut url = self.endpoint.clone();
        url.query_pairs_mut().append_pair("grant_type", grant);
        let mut response = self
            .client
            .post(url)
            .header("apikey", key)
            .json(&body)
            .send()
            .await
            .map_err(|_| ExchangeError::Unavailable)?;
        if [400, 401].contains(&response.status().as_u16()) {
            return Err(ExchangeError::Rejected);
        }
        if !response.status().is_success() {
            return Err(ExchangeError::Unavailable);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| ExchangeError::Unavailable)?
        {
            if bytes.len() + chunk.len() > 65536 {
                return Err(ExchangeError::Unavailable);
            }
            bytes.extend_from_slice(&chunk);
        }
        let reply: Reply =
            serde_json::from_slice(&bytes).map_err(|_| ExchangeError::Unavailable)?;
        if !header(&reply.access_token)
            || reply.refresh_token.is_empty()
            || reply.refresh_token.len() > 16384
            || !(1..=86400).contains(&reply.expires_in)
        {
            return Err(ExchangeError::Unavailable);
        }
        Ok(reply)
    }
}
#[cfg(test)]
mod tests;
