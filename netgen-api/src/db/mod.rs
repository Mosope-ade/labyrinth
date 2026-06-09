pub mod conversation_turns;
pub mod jobs;
pub mod projects;
pub mod schema;
pub mod users;

use sqlx::PgPool;

/// Initialise a PostgreSQL connection pool from `DATABASE_URL`.
/// Returns `None` gracefully if the var is unset — routes that need the DB
/// return 503 rather than crashing the server.
pub async fn connect() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    match PgPool::connect(&url).await {
        Ok(pool) => {
            tracing::info!("connected to PostgreSQL");
            Some(pool)
        }
        Err(e) => {
            tracing::error!("failed to connect to PostgreSQL: {e}");
            None
        }
    }
}
