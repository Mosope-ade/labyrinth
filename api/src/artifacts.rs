//! Private artifact download transport. No signed URLs or storage keys leave the API.
use reqwest::{Client, Url};
use std::time::Duration;
pub struct ArtifactReader {
    client: Client,
    origin: Url,
    bucket: String,
    auth: labyrinth_service_auth::Credentials,
}
impl ArtifactReader {
    pub fn new(
        origin: &str,
        bucket: String,
        api_key: String,
        token: String,
    ) -> Result<Self, &'static str> {
        let auth = labyrinth_service_auth::Credentials::fixed(api_key, token)
            .map_err(|_| "invalid artifact identity configuration")?;
        Self::build(origin, bucket, auth)
    }
    pub fn technical(
        origin: &str,
        bucket: String,
        api_key: String,
        email: String,
        password: String,
    ) -> Result<Self, &'static str> {
        let auth = labyrinth_service_auth::Credentials::password(origin, api_key, email, password)
            .map_err(|_| "invalid artifact identity configuration")?;
        Self::build(origin, bucket, auth)
    }
    fn build(
        origin: &str,
        bucket: String,
        auth: labyrinth_service_auth::Credentials,
    ) -> Result<Self, &'static str> {
        let origin = Url::parse(origin).map_err(|_| "invalid artifact storage configuration")?;
        if origin.scheme() != "https"
            || origin.host_str().is_none()
            || origin.path() != "/"
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
            || bucket.is_empty()
            || !bucket
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("invalid artifact storage configuration");
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| "artifact transport unavailable")?;
        Ok(Self {
            client,
            origin,
            bucket,
            auth,
        })
    }
    pub fn from_env() -> Result<Option<Self>, &'static str> {
        if std::env::var_os("LABYRINTH_ARTIFACT_STORAGE_TOKEN").is_some() {
            return Err(
                "static Storage token configuration is unsupported; use a technical identity",
            );
        };
        let names = [
            "LABYRINTH_ARTIFACT_STORAGE_ORIGIN",
            "LABYRINTH_ARTIFACT_STORAGE_BUCKET",
            "LABYRINTH_ARTIFACT_STORAGE_API_KEY",
            "LABYRINTH_ARTIFACT_STORAGE_EMAIL",
            "LABYRINTH_ARTIFACT_STORAGE_PASSWORD",
        ];
        let values: Vec<_> = names.iter().map(std::env::var).collect();
        if values
            .iter()
            .all(|v| matches!(v, Err(std::env::VarError::NotPresent)))
        {
            return Ok(None);
        }
        let values: Vec<_> = values
            .into_iter()
            .collect::<Result<_, _>>()
            .map_err(|_| "incomplete artifact storage configuration")?;
        Self::technical(
            &values[0],
            values[1].clone(),
            values[2].clone(),
            values[3].clone(),
            values[4].clone(),
        )
        .map(Some)
    }
    pub async fn read(&self, key: &str, size: u64) -> Result<Vec<u8>, ()> {
        if !(1..=32 * 1024 * 1024).contains(&size)
            || !key.starts_with("attempts/")
            || key.len() > 512
            || key
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
        {
            return Err(());
        }
        let url = self
            .origin
            .join(&format!(
                "storage/v1/object/authenticated/{}/{key}",
                self.bucket
            ))
            .map_err(|_| ())?;
        let mut response = self.auth.send(self.client.get(url)).await.map_err(|_| ())?;
        if !response.status().is_success() || response.content_length().is_some_and(|n| n > size) {
            return Err(());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
            if bytes.len() + chunk.len() > size as usize {
                return Err(());
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.len() != size as usize {
            return Err(());
        }
        Ok(bytes)
    }
}

#[cfg(all(test, feature = "postgres-tests"))]
impl ArtifactReader {
    pub(crate) fn for_test(origin: &str) -> Self {
        let mut reader = Self::new(
            "https://example.com",
            "private".into(),
            "public-test-key".into(),
            "test-only".into(),
        )
        .unwrap();
        reader.origin = Url::parse(origin).unwrap();
        reader
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reader_rejects_insecure_or_ambiguous_configuration() {
        for origin in [
            "http://example.com",
            "https://example.com/path",
            "https://user@example.com",
            "https://example.com/?token=x",
        ] {
            assert!(
                ArtifactReader::new(
                    origin,
                    "private".into(),
                    "public-test-key".into(),
                    "test-only".into()
                )
                .is_err()
            )
        }
        assert!(
            ArtifactReader::new(
                "https://example.com",
                "../other".into(),
                "public-test-key".into(),
                "test-only".into()
            )
            .is_err()
        );
    }
    #[tokio::test]
    async fn reader_sends_separate_gateway_key_and_bearer() {
        use axum::{Router, http::HeaderMap, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/storage/v1/object/authenticated/private/attempts/unit/bom.json",
            get(|headers: HeaderMap| async move {
                assert_eq!(headers["apikey"], "public-test-key");
                assert_eq!(headers["authorization"], "Bearer reader-token");
                "{}"
            }),
        );
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut reader = ArtifactReader::new(
            "https://example.com",
            "private".into(),
            "public-test-key".into(),
            "reader-token".into(),
        )
        .unwrap();
        reader.origin = Url::parse(&format!("http://{address}")).unwrap();
        assert_eq!(
            reader.read("attempts/unit/bom.json", 2).await.unwrap(),
            b"{}"
        );
        task.abort();
    }
}
