use sqlx::PgPool;
use uuid::Uuid;

use super::schema::ProjectRow;

const SELECT_COLS: &str =
    "id, user_id, name, session_id, intent_json, graph_json, status, created_at, updated_at";

pub async fn create(pool: &PgPool, user_id: Uuid, name: &str) -> sqlx::Result<ProjectRow> {
    sqlx::query_as::<_, ProjectRow>(&format!(
        "INSERT INTO projects (id, user_id, name, status, created_at, updated_at)
         VALUES ($1, $2, $3, 'draft', NOW(), NOW())
         RETURNING {SELECT_COLS}",
    ))
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(name)
    .fetch_one(pool)
    .await
}

/// Creates a project immediately from a completed intent, linking the chat session.
/// Status is set to 'draft' — a job should be queued to advance it to 'ready'.
pub async fn create_with_session(
    pool: &PgPool,
    user_id: Uuid,
    name: &str,
    session_id: Uuid,
    intent_json: serde_json::Value,
) -> sqlx::Result<ProjectRow> {
    sqlx::query_as::<_, ProjectRow>(&format!(
        "INSERT INTO projects (id, user_id, name, session_id, intent_json, status, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, 'draft', NOW(), NOW())
         RETURNING {SELECT_COLS}",
    ))
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(name)
    .bind(session_id)
    .bind(intent_json)
    .fetch_one(pool)
    .await
}

pub async fn get(pool: &PgPool, id: Uuid, user_id: Uuid) -> sqlx::Result<Option<ProjectRow>> {
    sqlx::query_as::<_, ProjectRow>(&format!(
        "SELECT {SELECT_COLS} FROM projects WHERE id = $1 AND user_id = $2",
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn list(pool: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<ProjectRow>> {
    sqlx::query_as::<_, ProjectRow>(&format!(
        "SELECT {SELECT_COLS} FROM projects WHERE user_id = $1 ORDER BY updated_at DESC",
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn update_graph(
    pool: &PgPool,
    id: Uuid,
    user_id: Uuid,
    intent_json: serde_json::Value,
    graph_json: serde_json::Value,
    status: &str,
) -> sqlx::Result<Option<ProjectRow>> {
    sqlx::query_as::<_, ProjectRow>(&format!(
        "UPDATE projects
         SET intent_json = $3, graph_json = $4, status = $5, updated_at = NOW()
         WHERE id = $1 AND user_id = $2
         RETURNING {SELECT_COLS}",
    ))
    .bind(id)
    .bind(user_id)
    .bind(intent_json)
    .bind(graph_json)
    .bind(status)
    .fetch_optional(pool)
    .await
}

pub async fn delete(pool: &PgPool, id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM projects WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn count_for_user(pool: &PgPool, user_id: Uuid) -> sqlx::Result<i64> {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM projects WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await?;
    Ok(row.0)
}
