//! Hosted client only. Credentials never cross into engine or API implementation crates.
mod auth;
mod callback;
mod config;
mod store;
pub use auth::Client;
pub use callback::Callback;
pub use config::Config;
pub use store::{Keychain, Vault};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("Invalid client configuration or input")]
    InvalidInput,
    #[error("Network/provider request failed; no mutation was automatically retried")]
    Unavailable,
    #[error("Authentication rejected; run laby login")]
    Unauthenticated,
    #[error("Provider response was invalid")]
    InvalidResponse,
    #[error("OS credential store unavailable or locked; no plaintext fallback is used")]
    CredentialStore,
    #[error("No saved login; run laby login")]
    NotLoggedIn,
    #[error("A login is already saved; run laby logout first")]
    AlreadyLoggedIn,
    #[error("Another laby command is using this profile; retry after it finishes")]
    Busy,
    #[error("Cannot bind loopback callback; check the selected port")]
    CallbackBind,
    #[error("Login expired or was cancelled; run laby login again")]
    LoginExpired,
    #[error("API rejected the request (HTTP {0}); check current access and input")]
    Api(u16),
    #[error("Remote logout was not confirmed; local credentials were erased")]
    LogoutIncomplete,
}

#[cfg(test)]
mod tests;
