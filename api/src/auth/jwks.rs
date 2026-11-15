use super::{AuthenticatedActor, Claims};
use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header,
    jwk::{AlgorithmParameters, EllipticCurve, JwkSet, KeyAlgorithm, KeyOperations, PublicKeyUse},
};
use reqwest::{Client, Url};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, RwLock};
const CACHE_TTL: Duration = Duration::from_secs(300);
const REFRESH_COOLDOWN: Duration = Duration::from_secs(30);
const MAX_JWKS_BYTES: usize = 65536;
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    #[error("invalid access token")]
    InvalidToken,
    #[error("authentication keys unavailable")]
    Unavailable,
    #[error("invalid authentication configuration")]
    Configuration,
}
#[derive(Clone)]
pub struct AuthConfig {
    issuer: String,
    url: Url,
}
impl AuthConfig {
    pub fn new(issuer: &str) -> Result<Self, AuthError> {
        let parsed = Url::parse(issuer).map_err(|_| AuthError::Configuration)?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || parsed.path() != "/auth/v1"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.as_str() != issuer
        {
            return Err(AuthError::Configuration);
        }
        let url = Url::parse(&format!("{issuer}/.well-known/jwks.json"))
            .map_err(|_| AuthError::Configuration)?;
        Ok(Self {
            issuer: issuer.into(),
            url,
        })
    }
    #[cfg(test)]
    pub(crate) fn loopback(issuer: &str, url: Url) -> Self {
        Self {
            issuer: issuer.into(),
            url,
        }
    }
}
#[derive(Clone)]
struct TrustedKey {
    algorithm: Algorithm,
    key: DecodingKey,
}
struct Cache {
    keys: BTreeMap<String, TrustedKey>,
    valid_until: Instant,
    next_refresh: Instant,
}
pub struct JwksVerifier {
    config: AuthConfig,
    client: Client,
    cache: RwLock<Cache>,
    refresh: Mutex<()>,
}
impl JwksVerifier {
    /// Bootstrap from the configured project only. Redirects and token-selected key
    /// URLs are never followed; startup fails if there is no trusted usable key set.
    pub async fn load(config: AuthConfig) -> Result<Self, AuthError> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| AuthError::Configuration)?;
        let keys = fetch(&client, &config.url).await?;
        let now = Instant::now();
        Ok(Self {
            config,
            client,
            cache: RwLock::new(Cache {
                keys,
                valid_until: now + CACHE_TTL,
                next_refresh: now + REFRESH_COOLDOWN,
            }),
            refresh: Mutex::new(()),
        })
    }
    pub async fn verify(&self, token: &str) -> Result<AuthenticatedActor, AuthError> {
        if token.len() > 16384 || token.is_empty() {
            return Err(AuthError::InvalidToken);
        }
        let header = decode_header(token).map_err(|_| AuthError::InvalidToken)?;
        if !matches!(header.alg, Algorithm::ES256 | Algorithm::RS256)
            || header.typ.as_deref().is_some_and(|t| t != "JWT")
            || header.crit.as_ref().is_some_and(|c| !c.is_empty())
        {
            return Err(AuthError::InvalidToken);
        }
        let kid = header
            .kid
            .filter(|k| !k.is_empty() && k.len() <= 128)
            .ok_or(AuthError::InvalidToken)?;
        let key = self.key(&kid).await?;
        if key.algorithm != header.alg {
            return Err(AuthError::InvalidToken);
        }
        let mut validation = Validation::new(key.algorithm);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&["authenticated"]);
        validation.set_required_spec_claims(&["sub", "exp", "iat", "iss", "aud"]);
        validation.leeway = 0;
        validation.validate_nbf = true;
        let claims = decode::<Claims>(token, &key.key, &validation)
            .map_err(|_| AuthError::InvalidToken)?
            .claims;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AuthError::Unavailable)?
            .as_secs();
        if claims.sub.is_nil()
            || claims.session_id.is_nil()
            || claims.role != "authenticated"
            || claims.is_anonymous
            || claims.iat > now.saturating_add(30)
            || claims.exp <= claims.iat
            || claims.exp <= now
        {
            return Err(AuthError::InvalidToken);
        }
        Ok(AuthenticatedActor {
            subject: claims.sub,
        })
    }
    async fn key(&self, kid: &str) -> Result<TrustedKey, AuthError> {
        {
            let cache = self.cache.read().await;
            if Instant::now() < cache.valid_until
                && let Some(key) = cache.keys.get(kid)
            {
                return Ok(key.clone());
            }
        }
        let _guard = self.refresh.lock().await;
        {
            let cache = self.cache.read().await;
            let now = Instant::now();
            if now < cache.valid_until
                && let Some(key) = cache.keys.get(kid)
            {
                return Ok(key.clone());
            }
            if now < cache.next_refresh {
                return Err(if now < cache.valid_until {
                    AuthError::InvalidToken
                } else {
                    AuthError::Unavailable
                });
            }
        }
        // Set the cooldown before I/O; errors cannot turn random kids into an
        // unbounded external-request stream. Existing fresh keys stay usable.
        self.cache.write().await.next_refresh = Instant::now() + REFRESH_COOLDOWN;
        let keys = fetch(&self.client, &self.config.url).await?;
        let mut cache = self.cache.write().await;
        cache.keys = keys;
        cache.valid_until = Instant::now() + CACHE_TTL;
        cache.keys.get(kid).cloned().ok_or(AuthError::InvalidToken)
    }
    #[cfg(test)]
    pub(crate) async fn expire_for_test(&self) {
        let mut c = self.cache.write().await;
        c.valid_until = Instant::now() - Duration::from_secs(1);
        c.next_refresh = c.valid_until;
    }
}
async fn fetch(client: &Client, url: &Url) -> Result<BTreeMap<String, TrustedKey>, AuthError> {
    let mut response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|_| AuthError::Unavailable)?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|n| n > MAX_JWKS_BYTES as u64)
    {
        return Err(AuthError::Unavailable);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| AuthError::Unavailable)? {
        if bytes.len() + chunk.len() > MAX_JWKS_BYTES {
            return Err(AuthError::Unavailable);
        }
        bytes.extend_from_slice(&chunk);
    }
    let set: JwkSet = serde_json::from_slice(&bytes).map_err(|_| AuthError::Unavailable)?;
    if set.keys.is_empty() || set.keys.len() > 16 {
        return Err(AuthError::Unavailable);
    }
    let mut keys = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    for jwk in set.keys {
        let kid = jwk
            .common
            .key_id
            .as_ref()
            .filter(|k| !k.is_empty() && k.len() <= 128)
            .ok_or(AuthError::Unavailable)?
            .clone();
        if !seen.insert(kid.clone()) {
            return Err(AuthError::Unavailable);
        }
        let algorithm = match jwk.common.key_algorithm {
            Some(KeyAlgorithm::ES256) => Algorithm::ES256,
            Some(KeyAlgorithm::RS256) => Algorithm::RS256,
            _ => continue,
        };
        if jwk
            .common
            .public_key_use
            .as_ref()
            .is_some_and(|u| u != &PublicKeyUse::Signature)
            || jwk
                .common
                .key_operations
                .as_ref()
                .is_some_and(|ops| !ops.contains(&KeyOperations::Verify))
        {
            return Err(AuthError::Unavailable);
        }
        let compatible = match (&jwk.algorithm, algorithm) {
            (AlgorithmParameters::EllipticCurve(p), Algorithm::ES256) => {
                p.curve == EllipticCurve::P256 && p.x.len() == 43 && p.y.len() == 43
            }
            (AlgorithmParameters::RSA(p), Algorithm::RS256) => {
                (342..=1366).contains(&p.n.len()) && (1..=8).contains(&p.e.len())
            }
            _ => false,
        };
        if !compatible {
            return Err(AuthError::Unavailable);
        }
        let key = DecodingKey::from_jwk(&jwk).map_err(|_| AuthError::Unavailable)?;
        keys.insert(kid, TrustedKey { algorithm, key });
    }
    if keys.is_empty() {
        return Err(AuthError::Unavailable);
    }
    Ok(keys)
}
