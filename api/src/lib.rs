//! Hosted application boundary: authenticated Axum admission/status and PostgreSQL persistence.
//! Job admission trusts only a server-authenticated actor; public clients cannot call
//! the database function. Generation remains in the separately executed pure engine.
pub mod jobs;
pub static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
pub mod auth;
pub mod http;
pub mod runtime;
