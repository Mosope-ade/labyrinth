//! Fenced hosted execution; pure network compilation remains in labyrinth-engine.
pub mod evidence;
pub mod package;
pub mod process;
pub mod runtime;
pub mod storage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid worker configuration or immutable input")]
    Invalid,
    #[error("worker lease lost or job cancelled")]
    LeaseLost,
    #[error("engine rejected requirements")]
    Deterministic,
    #[error("engine returned structured failure evidence")]
    Engine(Box<evidence::Evidence>),
    #[error("engine resource limit reached")]
    ResourceLimit,
    #[error("storage operation failed")]
    Storage,
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("engine process operation failed")]
    Io(#[from] std::io::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy)]
pub struct Limits {
    pub lease_secs: i32,
    pub heartbeat_secs: u64,
    pub deadline_secs: u64,
    pub cpu_secs: u64,
    pub memory_bytes: u64,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub attempts: i32,
    pub retry_first_secs: i32,
    pub retry_second_secs: i32,
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        if !(10..=300).contains(&self.lease_secs)
            || self.heartbeat_secs == 0
            || self
                .heartbeat_secs
                .checked_mul(3)
                .is_none_or(|n| n > self.lease_secs as u64)
            || !(1..=600).contains(&self.deadline_secs)
            || self.cpu_secs == 0
            || self.cpu_secs > self.deadline_secs
            || !(128 * 1024 * 1024..=8 * 1024 * 1024 * 1024).contains(&self.memory_bytes)
            || !(1..=2 * 1024 * 1024).contains(&self.input_bytes)
            || !(1..=32 * 1024 * 1024).contains(&self.output_bytes)
            || !(1..=3).contains(&self.attempts)
            || !(1..=3600).contains(&self.retry_first_secs)
            || !(self.retry_first_secs..=3600).contains(&self.retry_second_secs)
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handle {
    pub tenant_id: Uuid,
    pub job_id: Uuid,
    pub fence: Uuid,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub requirements: Value,
    pub provenance: Value,
    pub options: Value,
    pub catalog_yaml: String,
    pub catalog_sha256: String,
    pub base_revision_id: Option<Uuid>,
    pub base_manifest_sha256: Option<String>,
    pub staged_prefix: String,
    pub base_files: Vec<package::Object>,
}
pub async fn heartbeat(db: &sqlx::PgPool, h: &Handle, l: &Limits) -> Result<()> {
    let ok: bool = sqlx::query_scalar("SELECT labyrinth.heartbeat_attempt($1,$2,$3,$4)")
        .bind(h.tenant_id)
        .bind(h.job_id)
        .bind(h.fence)
        .bind(l.lease_secs)
        .fetch_one(db)
        .await?;
    if ok { Ok(()) } else { Err(Error::LeaseLost) }
}
pub mod cleanup;
pub mod retention;
