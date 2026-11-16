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
        match child() {
            Ok(()) => {}
            Err(Error::Engine(evidence)) => {
                let resource =
                    evidence.outcome == labyrinth_worker::evidence::Outcome::Inconclusive;
                let _ = serde_json::to_writer(std::io::stdout().lock(), &evidence);
                let _ = std::io::stdout().flush();
                std::process::exit(if resource { 3 } else { 2 });
            }
            Err(Error::ResourceLimit) => std::process::exit(3),
            Err(_) => std::process::exit(2),
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
    let cleanup_window = labyrinth_worker::cleanup::window_from_env()?;
    cfg.storage.verify_private().await?;
    let db = cfg.connect().await?;
    let cleanup_db = db.clone();
    let cleanup = async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            for _ in 0..16 {
                match labyrinth_worker::cleanup::once(&cleanup_db, &cfg.storage, cleanup_window)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(_) => {
                        eprintln!("orphan_cleanup_failed");
                        break;
                    }
                }
            }
        }
    };
    tokio::pin!(cleanup);
    loop {
        tokio::select! {
            _ = shutdown() => break,
            _ = &mut cleanup => break,
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
