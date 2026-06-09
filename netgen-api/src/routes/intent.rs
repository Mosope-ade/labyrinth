use axum::{extract::{ConnectInfo, State}, http::{HeaderMap, StatusCode}, Json};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use uuid::Uuid;

use crate::ai::{DesignPhase, Industry};
use crate::middleware::auth::{decode_token, Claims};
use crate::AppState;

// ─── POST /api/intent/chat ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ChatTurnRequest {
    pub session_id: Option<Uuid>,
    /// Provided by the frontend after the first project is created — links subsequent
    /// refining turns to the correct project row.
    pub project_id: Option<Uuid>,
    pub message: String,
}

#[derive(Serialize)]
pub struct ChatTurnResponse {
    pub session_id: Uuid,
    pub phase: DesignPhase,
    pub reply: String,
    pub intent_ready: bool,
    pub fallback_required: bool,
    pub detected_industry: Option<Industry>,
    /// Set when a project is created or when an existing project_id is in the request.
    pub project_id: Option<Uuid>,
}

pub async fn chat_turn(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<ChatTurnRequest>,
) -> Result<Json<ChatTurnResponse>, (StatusCode, String)> {
    if req.message.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "message must not be empty".into()));
    }

    let claims = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .and_then(|token| decode_token(token, &state.jwt_secret).ok());

    match &claims {
        Some(c) => {
            if let Some(user_id) = c.user_id() {
                state
                    .rate_limiter
                    .check(user_id, &c.tier)
                    .map_err(|retry_after| {
                        (
                            StatusCode::TOO_MANY_REQUESTS,
                            format!("rate limit exceeded — retry after {retry_after}s"),
                        )
                    })?;
            }
        }
        None => {
            state
                .ip_limiter
                .check(&addr.ip().to_string())
                .map_err(|retry_after| {
                    (
                        StatusCode::TOO_MANY_REQUESTS,
                        format!("rate limit exceeded — retry after {retry_after}s"),
                    )
                })?;
        }
    }

    let result = state
        .conversation_engine
        .turn(req.session_id, &req.message)
        .await
        .map_err(|e| {
            tracing::error!("conversation engine error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "An internal error occurred. Please try again.".into())
        })?;

    let intent_ready = result.intent.is_some();

    // ── DB persistence (authenticated users only) ─────────────────────────────
    let resolved_project_id = if let (Some(claims), Some(pool)) = (&claims, &state.db) {
        let user_id = match claims.user_id() {
            Some(id) => id,
            None => {
                return Ok(Json(ChatTurnResponse {
                    session_id: result.session_id,
                    phase: result.phase,
                    reply: result.reply,
                    intent_ready,
                    fallback_required: result.fallback_required,
                    detected_industry: result.detected_industry,
                    project_id: req.project_id,
                }))
            }
        };

        if intent_ready {
            // The AI produced a complete intent — create a project and capture history.
            let parsed = result.intent.as_ref().unwrap();

            let project_name = serde_json::to_value(&parsed.intent)
                .ok()
                .and_then(|v| v["metadata"]["name"].as_str().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    format!("Network Design {}", chrono::Utc::now().format("%b %d"))
                });

            let intent_json = serde_json::to_value(&parsed.intent).map_err(|e| {
                tracing::error!("failed to serialize intent: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "An internal error occurred. Please try again.".into())
            })?;

            match crate::db::projects::create_with_session(
                pool,
                user_id,
                &project_name,
                result.session_id,
                intent_json,
            )
            .await
            {
                Ok(project) => {
                    // Backfill the full conversation history into the new project.
                    if let Some(conv_state) = state.conversation_engine.state(result.session_id).await {
                        for msg in conv_state.user_turns() {
                            let role = format!("{:?}", msg.role).to_lowercase();
                            if let Err(e) = crate::db::conversation_turns::insert(
                                pool,
                                result.session_id,
                                Some(project.id),
                                Some(user_id),
                                &role,
                                &msg.content,
                            )
                            .await
                            {
                                tracing::warn!("failed to save conversation turn: {e}");
                            }
                        }
                    }
                    Some(project.id)
                }
                Err(e) => {
                    tracing::warn!("failed to create project from intent: {e}");
                    None
                }
            }
        } else if let Some(project_id) = req.project_id {
            // Refining phase — save the user message and AI reply to the existing project.
            if let Err(e) = crate::db::conversation_turns::insert(
                pool, result.session_id, Some(project_id), Some(user_id), "user", &req.message,
            ).await {
                tracing::warn!("failed to save user turn: {e}");
            }
            if let Err(e) = crate::db::conversation_turns::insert(
                pool, result.session_id, Some(project_id), Some(user_id), "assistant", &result.reply,
            ).await {
                tracing::warn!("failed to save assistant turn: {e}");
            }
            Some(project_id)
        } else {
            None
        }
    } else {
        req.project_id
    };

    Ok(Json(ChatTurnResponse {
        session_id: result.session_id,
        phase: result.phase,
        reply: result.reply,
        intent_ready,
        fallback_required: result.fallback_required,
        detected_industry: result.detected_industry,
        project_id: resolved_project_id,
    }))
}

// ─── POST /api/intent/submit ──────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct FormSubmitRequest {
    pub intent: serde_json::Value,
}

#[derive(Serialize)]
pub struct FormSubmitResponse {
    pub session_id: Uuid,
    pub valid: bool,
    pub errors: Vec<String>,
}

pub async fn form_submit(
    _claims: Claims,
    Json(req): Json<FormSubmitRequest>,
) -> Result<Json<FormSubmitResponse>, (StatusCode, String)> {
    use netgen_core::schema::NetworkIntent;
    use validator::Validate;

    let session_id = Uuid::new_v4();

    match serde_json::from_value::<NetworkIntent>(req.intent) {
        Ok(intent) => match intent.validate() {
            Ok(()) => Ok(Json(FormSubmitResponse {
                session_id,
                valid: true,
                errors: vec![],
            })),
            Err(e) => Ok(Json(FormSubmitResponse {
                session_id,
                valid: false,
                errors: e
                    .field_errors()
                    .into_iter()
                    .flat_map(|(field, errs)| {
                        errs.iter().map(move |ve| {
                            format!(
                                "{}: {}",
                                field,
                                ve.message.as_deref().unwrap_or("invalid value")
                            )
                        })
                    })
                    .collect(),
            })),
        },
        Err(e) => Ok(Json(FormSubmitResponse {
            session_id,
            valid: false,
            errors: vec![format!("Deserialization failed: {e}")],
        })),
    }
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<AppState>> {
    use axum::routing::post;
    axum::Router::new()
        .route("/chat", post(chat_turn))
        .route("/submit", post(form_submit))
}
