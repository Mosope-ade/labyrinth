//! Explicit privileged migration command. Never run under the runtime API login.
use sqlx::{
    ConnectOptions,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};
#[tokio::main]
async fn main() {
    if run().await.is_err() {
        eprintln!("Application migration failed; credentials and database details are redacted");
        std::process::exit(1)
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("LABYRINTH_MIGRATION_DATABASE_URL")?;
    let options = url
        .parse::<PgConnectOptions>()?
        .ssl_mode(PgSslMode::VerifyFull)
        .disable_statement_logging();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    labyrinth_api::MIGRATIONS.run(&pool).await?;
    println!("Application migrations applied");
    pool.close().await;
    Ok(())
}
