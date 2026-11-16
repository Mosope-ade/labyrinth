use super::PreparedAdmission;
use labyrinth_engine::intent::MissingField;
use serde::Serialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AdmissionError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("requirements failed validation")]
    InvalidRequirements(labyrinth_engine::diagnostics::ValidationErrors),
    #[error("requirements need clarification")]
    NeedsClarification(Vec<MissingField>),
    #[error("not permitted")]
    Forbidden,
    #[error("project not found")]
    NotFound,
    #[error("idempotency key reused with different input")]
    IdempotencyConflict,
    #[error("project head changed")]
    StaleHead,
    #[error("project recovery window expired")]
    RecoveryExpired,
    #[error("catalog unavailable for new admission")]
    CatalogUnavailable,
    #[error("invalid base revision or catalog mismatch")]
    InvalidBase,
    #[error("tenant active-job quota exceeded")]
    QuotaExceeded,
    #[error("database operation failed")]
    Database(#[source] sqlx::Error),
    #[error("request serialization failed")]
    Serialization(#[from] serde_json::Error),
}
impl From<sqlx::Error> for AdmissionError {
    fn from(error: sqlx::Error) -> Self {
        if let Some(db) = error.as_database_error()
            && db.code().as_deref() == Some("P0001")
        {
            return match db.message() {
                "admission_forbidden" => Self::Forbidden,
                "admission_not_found" => Self::NotFound,
                "admission_idempotency_conflict" => Self::IdempotencyConflict,
                "admission_recovery_expired" => Self::RecoveryExpired,
                "admission_stale_head" => Self::StaleHead,
                "admission_catalog_unavailable" => Self::CatalogUnavailable,
                "admission_invalid_base" => Self::InvalidBase,
                "admission_quota_exceeded" => Self::QuotaExceeded,
                _ => Self::Database(error),
            };
        }
        Self::Database(error)
    }
}
/// Durable job states; terminal records cannot be reused as new attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    RetryWait,
    CancelRequested,
    Cancelled,
    Failed,
    Succeeded,
}
#[derive(Debug, Serialize)]
pub struct AdmissionResult {
    pub job_id: Uuid,
    pub state: JobState,
    pub replayed: bool,
}
/// One autocommitted function statement owns authorization, locks, quota,
/// idempotency, job insert and pgmq.send in the same PostgreSQL transaction.
/// A connection failure may leave an unknown commit outcome: retry the SAME key.
/// Pool credentials must have only the labyrinth_api role's restricted privileges.
pub async fn admit(
    pool: &PgPool,
    request: &PreparedAdmission,
) -> Result<AdmissionResult, AdmissionError> {
    let c = &request.context;
    let row = sqlx::query("SELECT job_id, state, replayed FROM labyrinth.admit_job($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
        .bind(c.actor_id).bind(c.tenant_id).bind(c.project_id).bind(Uuid::new_v4())
        .bind(&request.key).bind(&request.digest).bind(&request.requirements).bind(&request.provenance)
        .bind(&request.options).bind(&request.catalog).bind(c.expected_head).bind(request.base)
        .fetch_one(pool).await?;
    Ok(AdmissionResult {
        job_id: row.try_get("job_id")?,
        state: row.try_get("state")?,
        replayed: row.try_get("replayed")?,
    })
}
