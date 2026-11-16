use labyrinth_worker::{
    Error,
    package::{self, EngineInput},
    runtime,
};
use std::io::{Read, Write};
fn child() -> Result<(), Error> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(Error::Invalid);
    }
    let input: EngineInput = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
    let files = package::generate(input)?;
    serde_json::to_writer(std::io::stdout().lock(), &files).map_err(|_| Error::Invalid)?;
    std::io::stdout().flush()?;
    Ok(())
}
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--engine-child") {
        if child().is_err() {
            std::process::exit(2)
        }
        return;
    }
    let result = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(Error::Io)
        .and_then(|rt| rt.block_on(serve()));
    if result.is_err() {
        eprintln!("worker_startup_failed");
        std::process::exit(1)
    }
}

async fn serve() -> Result<(), Error> {
    let cfg = runtime::Config::from_env()?;
    cfg.storage.verify_private().await?;
    let db = cfg.connect().await?;
    loop {
        tokio::select! {
            _ = shutdown() => break,
            result = runtime::once(&db, &cfg) => {
                match result {
                    Ok(true) => {},
                    Ok(false) => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
                    Err(_) => {
                        eprintln!("worker_poll_failed");
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }
    }
    db.close().await;
    Ok(())
}
async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {_ = terminate.recv() => {}, _ = tokio::signal::ctrl_c() => {}}
        } else {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
