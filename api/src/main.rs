use labyrinth_api::{auth::JwksVerifier, http, runtime::RuntimeConfig};
use std::sync::Arc;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::INFO)
        .init();
    let config = RuntimeConfig::from_env()?;
    let auth = Arc::new(JwksVerifier::load(config.auth.clone()).await?);
    let pool = config.connect().await?;
    let app = http::router(pool.clone(), auth, config.http)?;
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(event="api_started",address=%listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    pool.close().await;
    Ok(())
}
async fn shutdown() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
