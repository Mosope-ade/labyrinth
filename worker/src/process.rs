//! Linux prlimit applies hard limits before exec without unsafe pre_exec hooks.
use crate::{Error, Limits, Result};
use std::{path::Path, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};
/// Kill-on-drop also covers cancellation caused by heartbeat loss in the caller.
pub async fn run(
    executable: &Path,
    prlimit: &Path,
    input: Vec<u8>,
    limits: &Limits,
) -> Result<Vec<u8>> {
    limits.validate()?;
    if input.len() > limits.input_bytes {
        return Err(Error::Invalid);
    }
    let mut child = Command::new(prlimit)
        .arg(format!("--cpu={0}:{0}", limits.cpu_secs))
        .arg(format!("--as={0}:{0}", limits.memory_bytes))
        .arg("--core=0:0")
        .arg("--nofile=64:64")
        .arg("--")
        .arg(executable)
        .arg("--engine-child")
        .env_clear()
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or(Error::Invalid)?;
    let stdout = child.stdout.take().ok_or(Error::Invalid)?;
    // JSON Vec<u8> encoding is bounded at up to four wire bytes per package byte.
    let max_wire = limits.output_bytes.checked_mul(4).ok_or(Error::Invalid)?;
    let work = async {
        let write = async move {
            stdin.write_all(&input).await?;
            stdin.shutdown().await
        };
        let read = async {
            let mut bytes = Vec::new();
            stdout
                .take(max_wire as u64 + 1)
                .read_to_end(&mut bytes)
                .await?;
            Ok::<_, std::io::Error>(bytes)
        };
        let (_, out) = tokio::try_join!(write, read)?;
        if out.len() > max_wire {
            return Err(Error::ResourceLimit);
        }
        let status = child.wait().await?;
        if status.code() == Some(3) {
            return Err(Error::ResourceLimit);
        }
        if status.code() == Some(2) {
            return Err(Error::Deterministic);
        }
        if !status.success() {
            return Err(Error::ResourceLimit);
        }
        Ok(out)
    };
    match tokio::time::timeout(Duration::from_secs(limits.deadline_secs), work).await {
        Ok(result) => result,
        Err(_) => {
            child.kill().await?;
            let _ = child.wait().await;
            Err(Error::ResourceLimit)
        }
    }
}
