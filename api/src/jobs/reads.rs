use super::{AdmissionContext, AdmissionError, JobState};
use labyrinth_engine::catalog::{self, CatalogSnapshot};
use serde::Serialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Load only the authorized immutable catalog; never trust client-provided YAML.
pub async fn catalog_for_admission(
    pool: &PgPool,
    context: &AdmissionContext,
    hash: &str,
) -> Result<CatalogSnapshot, AdmissionError> {
    let yaml: String =
        sqlx::query_scalar("SELECT yaml FROM labyrinth.admission_catalog($1,$2,$3,$4)")
            .bind(context.actor_id)
            .bind(context.tenant_id)
            .bind(context.project_id)
            .bind(hash)
            .fetch_one(pool)
            .await?;
    let snapshot = catalog::load_yaml(&yaml)
        .map_err(|_| sqlx::Error::Protocol("invalid stored catalog".into()))?;
    if snapshot.version().sha256 != hash {
        return Err(sqlx::Error::Protocol("stored catalog identity mismatch".into()).into());
    }
    Ok(snapshot)
}
#[derive(Debug, Serialize)]
pub struct PublishedRevision {
    pub revision_id: Uuid,
    pub manifest_sha256: String,
}
#[derive(Debug, Serialize)]
pub struct JobStatus {
    pub job_id: Uuid,
    pub state: JobState,
    pub created_at: String,
    pub revision: Option<PublishedRevision>,
}
/// Authoritative scoped polling. No input, private object key or token is returned.
pub async fn status(
    pool: &PgPool,
    actor: Uuid,
    tenant: Uuid,
    project: Uuid,
    job: Uuid,
) -> Result<JobStatus, AdmissionError> {
    let row = sqlx::query("SELECT * FROM labyrinth.job_status($1,$2,$3,$4)")
        .bind(actor)
        .bind(tenant)
        .bind(project)
        .bind(job)
        .fetch_one(pool)
        .await?;
    let state: JobState = row.try_get("state")?;
    let revision: Option<Uuid> = row.try_get("revision_id")?;
    let digest: Option<String> = row.try_get("manifest_sha256")?;
    let revision = match (state, revision, digest) {
        (JobState::Succeeded, Some(revision_id), Some(manifest_sha256)) => {
            Some(PublishedRevision {
                revision_id,
                manifest_sha256,
            })
        }
        (JobState::Succeeded, _, _) => {
            return Err(
                sqlx::Error::Protocol("successful job has no published revision".into()).into(),
            );
        }
        (_, None, None) => None,
        _ => {
            return Err(sqlx::Error::Protocol("unexpected published revision state".into()).into());
        }
    };
    Ok(JobStatus {
        job_id: row.try_get("job_id")?,
        state,
        created_at: row.try_get("created_at")?,
        revision,
    })
}
