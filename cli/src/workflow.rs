//! Structured request transport. Raw JSON is preserved so the API can reject duplicate fields.
use crate::{Client, Error, Vault};
use reqwest::{Method, RequestBuilder};
use serde_json::Value;
use std::{io::Read, path::Path};

pub fn read_input(path: &Path) -> Result<Vec<u8>, Error> {
    let file = std::fs::File::open(path).map_err(|_| Error::InvalidInput)?;
    if !file.metadata().map_err(|_| Error::InvalidInput)?.is_file() {
        return Err(Error::InvalidInput);
    }
    let mut file = file.take(2 * 1024 * 1024 + 1);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| Error::InvalidInput)?;
    if bytes.len() > 2 * 1024 * 1024 || serde_json::from_slice::<Value>(&bytes).is_err() {
        return Err(Error::InvalidInput);
    }
    Ok(bytes)
}
impl Client {
    pub async fn submit(
        &self,
        vault: &dyn Vault,
        path: &str,
        body: Vec<u8>,
        key: Option<&str>,
    ) -> Result<Value, Error> {
        if body.len() > 2 * 1024 * 1024 || serde_json::from_slice::<Value>(&body).is_err() {
            return Err(Error::InvalidInput);
        }
        Self::check_key(key)?;
        let url = self.resource_url(path)?;
        let token = self.access(vault).await?;
        let mut request = self
            .http
            .post(url)
            .bearer_auth(token)
            .header("content-type", "application/json")
            .body(body);
        if let Some(key) = key {
            request = request.header("idempotency-key", key);
        }
        self.json_response(request).await
    }
    pub async fn page(
        &self,
        vault: &dyn Vault,
        path: &str,
        limit: u8,
        after: Option<&str>,
    ) -> Result<Value, Error> {
        if !(1..=100).contains(&limit)
            || after.is_some_and(|v| {
                v.len() > 64
                    || v.is_empty()
                    || !v.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
            })
        {
            return Err(Error::InvalidInput);
        }
        let url = self.resource_url(path)?;
        let token = self.access(vault).await?;
        let mut request = self
            .http
            .get(url)
            .bearer_auth(token)
            .query(&[("limit", limit.to_string())]);
        if let Some(after) = after {
            request = request.query(&[("after", after)]);
        }
        self.json_response(request).await
    }
    pub(crate) fn resource_url(&self, path: &str) -> Result<reqwest::Url, Error> {
        if !path.starts_with("v1/")
            || !path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
            || path.split('/').any(|p| p == ".." || p == ".")
        {
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
        Ok(url)
    }
    pub(crate) async fn json_response(&self, request: RequestBuilder) -> Result<Value, Error> {
        let response = request.send().await.map_err(|_| Error::Unavailable)?;
        if !response.status().is_success() {
            return Err(api_error(response).await);
        }
        serde_json::from_slice(&super::auth::bounded(response, 2 * 1024 * 1024).await?)
            .map_err(|_| Error::InvalidResponse)
    }
    pub(crate) async fn bytes(
        &self,
        token: &str,
        path: &str,
        max: usize,
    ) -> Result<Vec<u8>, Error> {
        let response = self
            .http
            .request(Method::GET, self.resource_url(path)?)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| Error::Unavailable)?;
        if !response.status().is_success() {
            return Err(api_error(response).await);
        }
        super::auth::bounded(response, max).await
    }
}

async fn api_error(response: reqwest::Response) -> Error {
    let status = response.status().as_u16();
    let Ok(bytes) = super::auth::bounded(response, 32768).await else {
        return Error::Api(status);
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return Error::Api(status);
    };
    let Some(code) = value.pointer("/error/code").and_then(Value::as_str) else {
        return Error::Api(status);
    };
    if code.is_empty()
        || code.len() > 64
        || !code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
    {
        return Error::Api(status);
    }
    let report=serde_json::json!({"code":code,"details":value.pointer("/error/details").unwrap_or(&Value::Null)}).to_string();
    if report.len() > 16384 {
        return Error::Api(status);
    }
    Error::ApiDetails { status, report }
}
