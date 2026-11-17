use crate::Error;
use reqwest::Url;
use sha2::{Digest, Sha256};
pub struct Config {
    pub(crate) api: Url,
    pub(crate) auth: Url,
    pub(crate) key: String,
}
impl Config {
    pub fn new(api: &str, auth: &str, key: &str) -> Result<Self, Error> {
        let api = root(api, true)?;
        let auth = root(auth, false)?;
        if !key.starts_with("sb_publishable_")
            || key.len() > 16384
            || reqwest::header::HeaderValue::from_str(key).is_err()
        {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            api,
            auth,
            key: key.to_owned(),
        })
    }
    pub fn profile_id(&self) -> String {
        format!(
            "{:x}",
            Sha256::digest(format!("{}\n{}", self.api, self.auth))
        )
    }
}
fn root(raw: &str, local: bool) -> Result<Url, Error> {
    let url = Url::parse(raw).map_err(|_| Error::InvalidInput)?;
    if (url.scheme() != "https"
        && !(local && url.scheme() == "http" && url.host_str() == Some("127.0.0.1")))
        || url.host_str().is_none()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::InvalidInput);
    }
    Ok(url)
}
