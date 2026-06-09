use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ─── Row types (map 1-to-1 with DB columns) ──────────────────────────────────

#[derive(Debug, Clone, FromRow, Deserialize)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub tier: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ProjectRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    /// In-memory session UUID that produced this project (for linking chat history).
    pub session_id: Option<Uuid>,
    /// JSON-serialised `NetworkIntent` — authoritative intent snapshot.
    pub intent_json: Option<serde_json::Value>,
    /// JSON-serialised `TopologyGraph` — authoritative graph snapshot.
    pub graph_json: Option<serde_json::Value>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct JobRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub job_type: String,
    /// `queued` | `running` | `complete` | `failed`
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ConversationTurnRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub project_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    /// `system` | `user` | `assistant`
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}
