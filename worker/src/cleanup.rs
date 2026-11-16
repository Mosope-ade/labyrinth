//! Bounded, repeatable deletion of terminal, unpublished attempt inventories only.
use crate::{Error, Result, package::NAMES};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;
pub trait OrphanStore {
    fn delete(&self, keys: &[String]) -> impl std::future::Future<Output = Result<()>> + Send;
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    fence: Uuid,
    token: Uuid,
    prefix: String,
}
pub fn window_from_env() -> Result<i32> {
    std::env::var("LABYRINTH_ORPHAN_SAFETY_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v| (86400..=2592000).contains(v))
        .ok_or(Error::Invalid)
}
pub async fn once(db: &PgPool, store: &(impl OrphanStore + Sync), window: i32) -> Result<bool> {
    if !(86400..=2592000).contains(&window) {
        return Err(Error::Invalid);
    }
    let claim: Option<serde_json::Value> = sqlx::query_scalar("SELECT labyrinth.claim_cleanup($1)")
        .bind(window)
        .fetch_one(db)
        .await?;
    let Some(claim) = claim else { return Ok(false) };
    let claim: Claim = serde_json::from_value(claim).map_err(|_| Error::Invalid)?;
    let prefix: Option<String> = sqlx::query_scalar("SELECT labyrinth.check_cleanup($1,$2)")
        .bind(claim.fence)
        .bind(claim.token)
        .fetch_one(db)
        .await?;
    if prefix.as_deref() != Some(&claim.prefix) {
        return Err(Error::LeaseLost);
    }
    let keys: Vec<_> = NAMES
        .iter()
        .map(|name| format!("{}{name}", claim.prefix))
        .collect();
    let result = store.delete(&keys).await;
    let finished: bool = sqlx::query_scalar("SELECT labyrinth.finish_cleanup($1,$2,$3)")
        .bind(claim.fence)
        .bind(claim.token)
        .bind(result.is_ok())
        .fetch_one(db)
        .await?;
    if !finished {
        return Err(Error::LeaseLost);
    }
    result.map(|_| true)
}
