//! Supabase asymmetric access-token verification; application authorization stays in Postgres.
mod jwks;
pub use jwks::{AuthConfig, AuthError, JwksVerifier};
use serde::Deserialize;
use uuid::Uuid;

/// Constructed only after signature and registered/session claim validation.
#[derive(Debug, Clone, Copy)]
pub struct AuthenticatedActor {
    subject: Uuid,
}
impl AuthenticatedActor {
    pub fn id(&self) -> Uuid {
        self.subject
    }
}
#[derive(Deserialize)]
struct Claims {
    sub: Uuid,
    exp: u64,
    iat: u64,
    role: String,
    session_id: Uuid,
    is_anonymous: bool,
}
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
