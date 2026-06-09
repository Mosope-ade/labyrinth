use axum::{extract::{ConnectInfo, State}, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};

use crate::{
    middleware::auth::{encode_token, Claims, UserTier},
    AppState,
};

fn is_valid_email(email: &str) -> bool {
    let mut parts = email.splitn(2, '@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

// bcrypt silently truncates passwords at 72 bytes — enforce this as a hard max
// to prevent callers submitting megabyte-sized strings to exhaust CPU.
const PASSWORD_MAX_BYTES: usize = 72;
const PASSWORD_MIN_LEN: usize = 8;

// ─── POST /api/auth/register ──────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct RegisterResponse {
    pub user_id: String,
    pub email: String,
    pub token: String,
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>), (StatusCode, Json<serde_json::Value>)> {
    let pool = state.db.as_ref().ok_or_else(db_unavailable)?;

    // IP-based rate limit — 5 registrations per IP per hour
    state
        .register_limiter
        .check(&addr.ip().to_string())
        .map_err(|retry_after| {
            (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": format!("too many registration attempts — retry after {retry_after}s")
                })),
            )
        })?;

    let email = req.email.trim().to_string();

    if email.is_empty() {
        return Err(bad_request("email is required"));
    }
    if !is_valid_email(&email) {
        return Err(bad_request("invalid email address"));
    }
    if req.password.len() < PASSWORD_MIN_LEN {
        return Err(bad_request("password must be at least 8 characters"));
    }
    if req.password.len() > PASSWORD_MAX_BYTES {
        return Err(bad_request("password must not exceed 72 characters"));
    }

    if crate::db::users::find_by_email(pool, &email)
        .await
        .map_err(db_error)?
        .is_some()
    {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "email already registered"})),
        ));
    }

    let hash = bcrypt::hash(&req.password, bcrypt::DEFAULT_COST)
        .map_err(|e| internal_error(&e.to_string()))?;

    let user = crate::db::users::create(pool, &email, &hash, "free")
        .await
        .map_err(db_error)?;

    let claims = Claims::new(user.id, &user.email, UserTier::Free, 86400 * 7); // 7 days
    let token = encode_token(&claims, &state.jwt_secret)
        .map_err(|e| internal_error(&e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(RegisterResponse {
            user_id: user.id.to_string(),
            email: user.email,
            token,
        }),
    ))
}

// ─── POST /api/auth/login ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub user_id: String,
    pub email: String,
    pub tier: String,
    pub token: String,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<serde_json::Value>)> {
    let pool = state.db.as_ref().ok_or_else(db_unavailable)?;

    // Per-IP rate limit — 10 attempts per 15 minutes
    state
        .login_limiter
        .check(&addr.ip().to_string())
        .map_err(|retry_after| {
            (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": format!("too many login attempts — retry after {retry_after}s")
                })),
            )
        })?;

    // Enforce password length before touching bcrypt (CPU DoS guard)
    if req.password.len() > PASSWORD_MAX_BYTES {
        // Return the same error as wrong password to avoid user enumeration
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid credentials"})),
        ));
    }

    let user = crate::db::users::find_by_email(pool, &req.email)
        .await
        .map_err(db_error)?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "invalid credentials"})),
            )
        })?;

    let valid = bcrypt::verify(&req.password, &user.password_hash)
        .map_err(|e| internal_error(&e.to_string()))?;

    if !valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid credentials"})),
        ));
    }

    let tier = UserTier::parse_tier(&user.tier);
    let claims = Claims::new(user.id, &user.email, tier.clone(), 86400 * 7); // 7 days
    let token = encode_token(&claims, &state.jwt_secret)
        .map_err(|e| internal_error(&e.to_string()))?;

    Ok(Json(LoginResponse {
        user_id: user.id.to_string(),
        email: user.email,
        tier: format!("{:?}", tier).to_lowercase(),
        token,
    }))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn bad_request(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": msg})))
}

fn db_unavailable() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"error": "database not configured"})),
    )
}

fn db_error(e: sqlx::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("database error: {e}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "internal server error"})),
    )
}

fn internal_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("internal error: {msg}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "internal server error"})),
    )
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn router() -> axum::Router<Arc<AppState>> {
    use axum::routing::post;
    axum::Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
}
