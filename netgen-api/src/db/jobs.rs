use sqlx::PgPool;
use uuid::Uuid;

use super::schema::JobRow;

pub async fn create(
    pool: &PgPool,
    project_id: Uuid,
    job_type: &str,
) -> sqlx::Result<JobRow> {
    sqlx::query_as::<_, JobRow>(
        "INSERT INTO jobs (id, project_id, job_type, status, created_at, updated_at)
         VALUES ($1, $2, $3, 'queued', NOW(), NOW())
         RETURNING id, project_id, job_type, status, error_message, created_at, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(job_type)
    .fetch_one(pool)
    .await
}

pub async fn update_status(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    error_message: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE jobs SET status = $1, error_message = $2, updated_at = NOW() WHERE id = $3",
    )
    .bind(status)
    .bind(error_message)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<JobRow>> {
    sqlx::query_as::<_, JobRow>(
        "SELECT id, project_id, job_type, status, error_message, created_at, updated_at
         FROM jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn list_for_project(pool: &PgPool, project_id: Uuid) -> sqlx::Result<Vec<JobRow>> {
    sqlx::query_as::<_, JobRow>(
        "SELECT id, project_id, job_type, status, error_message, created_at, updated_at
         FROM jobs WHERE project_id = $1 ORDER BY created_at DESC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
}
