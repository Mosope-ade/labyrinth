use std::sync::Arc;

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
    Json,
};
use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

// ─── UserTier ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserTier {
    Free,
    Pro,
    Enterprise,
}

impl UserTier {
    /// Max AI conversation turns per hour. 0 = unlimited.
    pub fn ai_calls_per_hour(&self) -> usize {
        match self {
            UserTier::Free => 5,
            UserTier::Pro => 100,
            UserTier::Enterprise => 0,
        }
    }

    /// Max concurrent projects.
    pub fn max_projects(&self) -> usize {
        match self {
            UserTier::Free => 3,
            UserTier::Pro => 50,
            UserTier::Enterprise => 0, // unlimited
        }
    }

    pub fn parse_tier(s: &str) -> Self {
        match s {
            "pro" => UserTier::Pro,
            "enterprise" => UserTier::Enterprise,
            _ => UserTier::Free,
        }
    }
}

// ─── Claims ───────────────────────────────────────────────────────────────────

/// JWT payload embedded in every authenticated request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the user's UUID.
    pub sub: String,
    pub email: String,
    pub tier: UserTier,
    /// Expiry as Unix timestamp.
    pub exp: usize,
    /// Issued-at as Unix timestamp.
    pub iat: usize,
    /// Not-before as Unix timestamp — prevents token use before activation.
    pub nbf: usize,
}

impl Claims {
    pub fn new(user_id: Uuid, email: impl Into<String>, tier: UserTier, ttl_secs: u64) -> Self {
        let now = Utc::now().timestamp() as usize;
        Self {
            sub: user_id.to_string(),
            email: email.into(),
            tier,
            iat: now,
            nbf: now,
            exp: now + ttl_secs as usize,
        }
    }

    pub fn user_id(&self) -> Option<Uuid> {
        self.sub.parse().ok()
    }
}

// ─── Token operations ─────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Token creation failed: {0}")]
    TokenCreation(String),
    #[error("Token has expired")]
    TokenExpired,
    #[error("Token is invalid: {0}")]
    TokenInvalid(String),
    #[error("Authorization header missing or malformed")]
    MissingToken,
}

/// Encodes `claims` into a signed JWT string.
pub fn encode_token(claims: &Claims, secret: &str) -> Result<String, AuthError> {
    jsonwebtoken::encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AuthError::TokenCreation(e.to_string()))
}

/// Decodes and validates a JWT string, returning the embedded claims.
pub fn decode_token(token: &str, secret: &str) -> Result<Claims, AuthError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_nbf = true;
    let data = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|e| {
        use jsonwebtoken::errors::ErrorKind;
        match e.kind() {
            ErrorKind::ExpiredSignature => AuthError::TokenExpired,
            _ => AuthError::TokenInvalid(e.to_string()),
        }
    })?;
    Ok(data.claims)
}

// ─── Axum extractor ───────────────────────────────────────────────────────────

/// Extracts and validates JWT claims from the `Authorization: Bearer <token>` header.
///
/// Use as a handler parameter to require authentication:
/// ```ignore
/// async fn handler(claims: Claims, ...) -> impl IntoResponse { ... }
/// ```
impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
    Arc<crate::AppState>: FromRef<S>,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = Arc::<crate::AppState>::from_ref(state);

        let token = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or_else(|| auth_rejection("Missing or malformed Authorization header"))?;

        decode_token(token, &app_state.jwt_secret)
            .map_err(|e| auth_rejection(&e.to_string()))
    }
}

fn auth_rejection(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": msg})),
    )
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-key-long-enough-for-hs256";

    fn test_claims(tier: UserTier, ttl: u64) -> Claims {
        Claims::new(Uuid::new_v4(), "user@example.com", tier, ttl)
    }

    #[test]
    fn encode_decode_round_trips() {
        let claims = test_claims(UserTier::Pro, 3600);
        let token = encode_token(&claims, SECRET).expect("encode failed");
        let decoded = decode_token(&token, SECRET).expect("decode failed");
        assert_eq!(decoded.email, "user@example.com");
        assert_eq!(decoded.tier, UserTier::Pro);
    }

    #[test]
    fn decoded_user_id_matches_original() {
        let uid = Uuid::new_v4();
        let claims = Claims::new(uid, "a@b.com", UserTier::Free, 3600);
        let token = encode_token(&claims, SECRET).unwrap();
        let decoded = decode_token(&token, SECRET).unwrap();
        assert_eq!(decoded.user_id(), Some(uid));
    }

    #[test]
    fn expired_token_is_rejected() {
        let claims = test_claims(UserTier::Free, 0); // expires immediately
        // Build a token with exp in the past by manipulating iat/exp
        let mut c = claims;
        c.exp = 1; // Unix timestamp 1970-01-01 — definitely expired
        let token = encode_token(&c, SECRET).unwrap();
        let err = decode_token(&token, SECRET).unwrap_err();
        assert!(matches!(err, AuthError::TokenExpired));
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let claims = test_claims(UserTier::Free, 3600);
        let token = encode_token(&claims, SECRET).unwrap();
        let err = decode_token(&token, "wrong-secret").unwrap_err();
        assert!(matches!(err, AuthError::TokenInvalid(_)));
    }

    #[test]
    fn garbage_token_is_rejected() {
        let err = decode_token("not.a.jwt", SECRET).unwrap_err();
        assert!(matches!(err, AuthError::TokenInvalid(_)));
    }

    #[test]
    fn tier_ai_limits_are_ordered() {
        assert!(UserTier::Free.ai_calls_per_hour() < UserTier::Pro.ai_calls_per_hour());
        assert_eq!(UserTier::Enterprise.ai_calls_per_hour(), 0); // unlimited
    }

    #[test]
    fn tier_project_limits_are_ordered() {
        assert!(UserTier::Free.max_projects() < UserTier::Pro.max_projects());
        assert_eq!(UserTier::Enterprise.max_projects(), 0);
    }

    #[test]
    fn tier_parse_tier_parses_known_values() {
        assert_eq!(UserTier::parse_tier("pro"), UserTier::Pro);
        assert_eq!(UserTier::parse_tier("enterprise"), UserTier::Enterprise);
        assert_eq!(UserTier::parse_tier("unknown"), UserTier::Free);
    }

    #[test]
    fn auth_error_messages_are_descriptive() {
        assert!(AuthError::TokenExpired.to_string().contains("expired"));
        assert!(AuthError::MissingToken.to_string().contains("Authorization"));
        assert!(AuthError::TokenCreation("x".into()).to_string().contains("creation"));
    }

    #[test]
    fn claims_new_sets_iat_before_exp() {
        let c = test_claims(UserTier::Free, 3600);
        assert!(c.exp > c.iat);
        assert_eq!(c.exp - c.iat, 3600);
    }
}
