//! Policy-bound erasure, separate from unpublished-orphan cleanup.
use crate::{Error, Result, cleanup::OrphanStore, package::NAMES};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    fence: Uuid,
    token: Uuid,
    prefix: String,
}
/// One bounded pass. Database tombstones, not caller-controlled keys, authorize deletion.
pub async fn once(db: &PgPool, store: &(impl OrphanStore + Sync)) -> Result<bool> {
    let retired: i32 = sqlx::query_scalar("SELECT labyrinth.retire_project_jobs(NULL,NULL,100)")
        .fetch_one(db)
        .await?;
    let acknowledged: i32 = sqlx::query_scalar("SELECT labyrinth.ack_terminal_messages(100)")
        .fetch_one(db)
        .await?;
    let pruned: i32 = sqlx::query_scalar("SELECT labyrinth.prune_job_payloads(100)")
        .fetch_one(db)
        .await?;
    let completed: bool = sqlx::query_scalar("SELECT labyrinth.finish_project_tombstone()")
        .fetch_one(db)
        .await?;
    let claim: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT labyrinth.claim_project_erasure()")
            .fetch_one(db)
            .await?;
    let Some(claim) = claim else {
        return Ok(retired > 0 || acknowledged > 0 || pruned > 0 || completed);
    };
    let claim: Claim = serde_json::from_value(claim).map_err(|_| Error::Invalid)?;
    let prefix: Option<String> =
        sqlx::query_scalar("SELECT labyrinth.check_project_erasure($1,$2)")
            .bind(claim.fence)
            .bind(claim.token)
            .fetch_one(db)
            .await?;
    if prefix.as_deref() != Some(&claim.prefix) {
        return Err(Error::LeaseLost);
    }
    let keys: Vec<_> = NAMES
        .iter()
        .map(|n| format!("{}{n}", claim.prefix))
        .collect();
    let result = store.delete(&keys).await;
    let finished: bool = sqlx::query_scalar("SELECT labyrinth.finish_project_erasure($1,$2,$3)")
        .bind(claim.fence)
        .bind(claim.token)
        .bind(result.is_ok())
        .fetch_one(db)
        .await?;
    if !finished {
        return Err(Error::LeaseLost);
    }
    result?;
    let _: bool = sqlx::query_scalar("SELECT labyrinth.finish_project_tombstone()")
        .fetch_one(db)
        .await?;
    Ok(true)
}
