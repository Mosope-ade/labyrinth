//! Private Supabase Storage. Never overwrite staged objects or follow redirects.
use crate::{Error, Result};
use reqwest::{Client, Url};
use std::time::Duration;
pub struct Storage {
    client: Client,
    base: Url,
    bucket: String,
    token: String,
}
impl Storage {
    pub fn new(origin: &str, bucket: String, token: String) -> Result<Self> {
        let mut base = Url::parse(origin).map_err(|_| Error::Invalid)?;
        if base.scheme() != "https"
            || base.host_str().is_none()
            || base.path() != "/"
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || bucket.is_empty()
            || !bucket
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || token.is_empty()
        {
            return Err(Error::Invalid);
        }
        base.set_path("/storage/v1/object/");
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| Error::Storage)?;
        Ok(Self {
            client,
            base,
            bucket,
            token,
        })
    }
    pub async fn verify_private(&self) -> Result<()> {
        let url = self
            .base
            .join(&format!("../bucket/{}", self.bucket))
            .map_err(|_| Error::Invalid)?;
        let mut response = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .header("apikey", &self.token)
            .send()
            .await
            .map_err(|_| Error::Storage)?;
        if !response.status().is_success() {
            return Err(Error::Storage);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Storage)? {
            if bytes.len() + chunk.len() > 65536 {
                return Err(Error::Storage);
            }
            bytes.extend_from_slice(&chunk);
        }
        let metadata: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| Error::Storage)?;
        if metadata["public"] != false || metadata["id"] != self.bucket {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    fn url(&self, key: &str) -> Result<Url> {
        if !key.starts_with("attempts/")
            || key.len() > 512
            || key
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
        {
            return Err(Error::Invalid);
        }
        self.base
            .join(&format!("{}/{key}", self.bucket))
            .map_err(|_| Error::Invalid)
    }
    pub async fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let response = self
            .client
            .post(self.url(key)?)
            .bearer_auth(&self.token)
            .header("apikey", &self.token)
            .header("Content-Type", "application/json")
            .header("x-upsert", "false")
            .body(bytes.to_vec())
            .send()
            .await
            .map_err(|_| Error::Storage)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Error::Storage)
        }
    }
    pub async fn get(&self, key: &str, limit: usize) -> Result<Vec<u8>> {
        self.url(key)?;
        let mut response = self
            .client
            .get(
                self.base
                    .join(&format!("authenticated/{}/{key}", self.bucket))
                    .map_err(|_| Error::Invalid)?,
            )
            .bearer_auth(&self.token)
            .header("apikey", &self.token)
            .send()
            .await
            .map_err(|_| Error::Storage)?;
        if !response.status().is_success()
            || response.content_length().is_some_and(|n| n > limit as u64)
        {
            return Err(Error::Storage);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Storage)? {
            if bytes.len() + chunk.len() > limit {
                return Err(Error::Storage);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

/// Storage port permits fault-injection tests without weakening production HTTPS.
pub trait ObjectStore {
    fn put(&self, key: &str, bytes: &[u8]) -> impl std::future::Future<Output = Result<()>> + Send;
    fn get(
        &self,
        key: &str,
        limit: usize,
    ) -> impl std::future::Future<Output = Result<Vec<u8>>> + Send;
}
impl ObjectStore for Storage {
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        Storage::put(self, key, bytes).await
    }
    async fn get(&self, key: &str, limit: usize) -> Result<Vec<u8>> {
        Storage::get(self, key, limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Bytes,
        extract::State,
        http::{HeaderMap, StatusCode},
        routing::{get, post},
    };
    use std::sync::{Arc, Mutex};
    #[test]
    fn origins_and_paths_fail_closed() {
        for origin in [
            "http://example.com",
            "https://user:pass@example.com",
            "https://example.com/other",
            "https://example.com/?secret=x",
        ] {
            assert!(Storage::new(origin, "private".into(), "test".into()).is_err())
        }
        let s = Storage::new("https://example.com", "private".into(), "test".into()).unwrap();
        for key in [
            "attempts/../secret",
            "/attempts/a",
            "attempts/a%2fb",
            "attempts/a//b",
        ] {
            assert!(s.url(key).is_err())
        }
    }
    #[tokio::test]
    async fn storage_http_headers_private_route_errors_and_stream_bounds() {
        async fn upload(
            State(state): State<Arc<Mutex<Vec<u8>>>>,
            headers: HeaderMap,
            bytes: Bytes,
        ) -> StatusCode {
            assert_eq!(headers["authorization"], "Bearer test-only");
            assert_eq!(headers["apikey"], "test-only");
            assert_eq!(headers["x-upsert"], "false");
            *state.lock().unwrap() = bytes.to_vec();
            StatusCode::OK
        }
        async fn download(State(state): State<Arc<Mutex<Vec<u8>>>>) -> Vec<u8> {
            state.lock().unwrap().clone()
        }
        let state = Arc::new(Mutex::new(Vec::new()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route(
                "/storage/v1/object/private/attempts/a/file.json",
                post(upload),
            )
            .route(
                "/storage/v1/object/authenticated/private/attempts/a/file.json",
                get(download),
            )
            .route(
                "/storage/v1/bucket/private",
                get(|| async { axum::Json(serde_json::json!({"id":"private","public":false})) }),
            )
            .with_state(state);
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        // Test-only local transport; production constructor still requires HTTPS.
        let mut s =
            Storage::new("https://example.com", "private".into(), "test-only".into()).unwrap();
        s.base = Url::parse(&format!("http://{addr}/storage/v1/object/")).unwrap();
        s.verify_private().await.unwrap();
        s.put("attempts/a/file.json", b"payload").await.unwrap();
        assert_eq!(s.get("attempts/a/file.json", 7).await.unwrap(), b"payload");
        assert!(s.get("attempts/a/file.json", 6).await.is_err());
        assert!(s.put("attempts/a/missing.json", b"x").await.is_err());
        task.abort();
    }
}
