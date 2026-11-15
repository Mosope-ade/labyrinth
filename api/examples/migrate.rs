//! Explicit privileged migration command. Never run under the runtime API login.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("LABYRINTH_MIGRATION_DATABASE_URL")?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    labyrinth_api::MIGRATIONS.run(&pool).await?;
    println!("Application migrations applied");
    pool.close().await;
    Ok(())
}
