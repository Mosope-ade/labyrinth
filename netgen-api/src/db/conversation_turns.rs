use sqlx::PgPool;
use uuid::Uuid;

use super::schema::ConversationTurnRow;

pub async fn insert(
    pool: &PgPool,
    session_id: Uuid,
    project_id: Option<Uuid>,
    user_id: Option<Uuid>,
    role: &str,
    content: &str,
) -> sqlx::Result<ConversationTurnRow> {
    sqlx::query_as::<_, ConversationTurnRow>(
        "INSERT INTO conversation_turns (id, session_id, project_id, user_id, role, content, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW())
         RETURNING id, session_id, project_id, user_id, role, content, created_at",
    )
    .bind(Uuid::new_v4())
    .bind(session_id)
    .bind(project_id)
    .bind(user_id)
    .bind(role)
    .bind(content)
    .fetch_one(pool)
    .await
}

/// Returns all turns for a project, scoped by user ownership.
/// The join through `projects` prevents users from reading each other's history.
pub async fn list_by_project(
    pool: &PgPool,
    project_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Vec<ConversationTurnRow>> {
    sqlx::query_as::<_, ConversationTurnRow>(
        "SELECT ct.id, ct.session_id, ct.project_id, ct.user_id, ct.role, ct.content, ct.created_at
         FROM conversation_turns ct
         JOIN projects p ON p.id = ct.project_id
         WHERE ct.project_id = $1 AND p.user_id = $2
         ORDER BY ct.created_at ASC",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
}
