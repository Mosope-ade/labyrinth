use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::{middleware::auth::decode_token, AppState};

#[derive(Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

/// GET /api/projects/:id/stream?token=<jwt>
///
/// Validates the JWT before upgrading. Rejects unauthenticated connections.
pub async fn stream_project(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<Uuid>,
    Query(query): Query<TokenQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let token = query.token.as_deref().ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "token query parameter is required"})),
        )
    })?;

    let claims = decode_token(token, &state.jwt_secret).map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid or expired token"})),
        )
    })?;

    let user_id = claims.user_id().ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid token subject"})),
        )
    })?;

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, project_id, user_id)))
}

async fn handle_socket(
    mut socket: WebSocket,
    state: Arc<AppState>,
    project_id: Uuid,
    _user_id: Uuid,
) {
    // TODO(Phase 12): verify user owns project_id before subscribing.
    let mut rx = state.job_queue.subscribe(project_id).await;

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        let text = match serde_json::to_string(&event) {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::warn!("failed to serialize job event: {e}");
                                continue;
                            }
                        };
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("ws subscriber lagged by {n} messages for project {project_id}");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(payload))) => {
                        let _ = socket.send(Message::Pong(payload)).await;
                    }
                    _ => {}
                }
            }
        }
    }
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<AppState>> {
    use axum::routing::get;
    axum::Router::new().route("/{id}/stream", get(stream_project))
}
