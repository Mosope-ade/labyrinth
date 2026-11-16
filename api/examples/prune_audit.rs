//! Explicit maintenance-only retention tool; never run using API/worker credentials.
use sqlx::{
    ConnectOptions,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};
#[tokio::main]
async fn main() {
    if run().await.is_err() {
        eprintln!("Audit retention failed; credentials and database details are redacted");
        std::process::exit(1)
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("LABYRINTH_MAINTENANCE_DATABASE_URL")?;
    let batch = std::env::args()
        .nth(1)
        .map(|v| v.parse::<i32>())
        .transpose()?
        .unwrap_or(1000);
    if !(1..=1000).contains(&batch) {
        return Err("invalid batch".into());
    }
    let options = url
        .parse::<PgConnectOptions>()?
        .ssl_mode(PgSslMode::VerifyFull)
        .disable_statement_logging();
    let pool=PgPoolOptions::new().max_connections(1).after_connect(|c,_|Box::pin(async move{
        let privileged:bool=sqlx::query_scalar("SELECT rolsuper OR rolcreatedb OR rolcreaterole OR rolbypassrls FROM pg_catalog.pg_roles WHERE rolname=session_user").fetch_one(&mut *c).await?;
        if privileged{return Err(sqlx::Error::Protocol("maintenance requires a restricted login".into()))}
        sqlx::query("SET ROLE labyrinth_maintenance").execute(&mut *c).await?;
        sqlx::query("SET statement_timeout='5s'").execute(&mut *c).await?;sqlx::query("SET lock_timeout='2s'").execute(&mut *c).await?;Ok(())
    })).connect_with(options).await?;
    let removed: i64 = sqlx::query_scalar("SELECT labyrinth.prune_admin_audit($1)")
        .bind(batch)
        .fetch_one(&pool)
        .await?;
    println!("Pruned {removed} expired audit records");
    pool.close().await;
    Ok(())
}
