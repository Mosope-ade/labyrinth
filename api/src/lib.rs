//! Hosted application boundary. No HTTP listener or token verifier is implemented yet.
//! Job admission trusts only a server-authenticated actor; public clients cannot call
//! the database function. Generation remains in the separately executed pure engine.
pub mod jobs;
pub static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
