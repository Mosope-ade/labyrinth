//! Private artifact download transport. No signed URLs or storage keys leave the API.
use reqwest::{Client, Url};
use std::time::Duration;
pub struct ArtifactReader {
    client: Client,
    origin: Url,
    bucket: String,
    token: String,
}
impl ArtifactReader {
    pub fn new(origin: &str, bucket: String, token: String) -> Result<Self, &'static str> {
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
            || token.is_empty()
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
            token,
        })
    }
    pub fn from_env() -> Result<Option<Self>, &'static str> {
        let names = [
            "LABYRINTH_ARTIFACT_STORAGE_ORIGIN",
            "LABYRINTH_ARTIFACT_STORAGE_BUCKET",
            "LABYRINTH_ARTIFACT_STORAGE_TOKEN",
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
        Self::new(&values[0], values[1].clone(), values[2].clone()).map(Some)
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
        let mut response = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .header("apikey", &self.token)
            .send()
            .await
            .map_err(|_| ())?;
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

#[cfg(test)]
impl ArtifactReader {
    pub(crate) fn for_test(origin: &str) -> Self {
        let mut reader =
            Self::new("https://example.com", "private".into(), "test-only".into()).unwrap();
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
            assert!(ArtifactReader::new(origin, "private".into(), "test-only".into()).is_err())
        }
        assert!(
            ArtifactReader::new("https://example.com", "../other".into(), "test-only".into())
                .is_err()
        );
    }
}
