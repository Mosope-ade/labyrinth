pub mod ai;
pub mod db;
pub mod jobs;
pub mod middleware;
pub mod routes;

use std::sync::Arc;

use axum::{Router, http::header};
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;

use crate::ai::ConversationEngine;
use crate::jobs::JobQueue;
use crate::middleware::{AuthRateLimiter, RateLimiter};

// ─── AppState ─────────────────────────────────────────────────────────────────

pub struct AppState {
    pub conversation_engine: ConversationEngine,
    pub db: Option<PgPool>,
    pub job_queue: Arc<JobQueue>,
    pub jwt_secret: String,
    pub rate_limiter: Arc<RateLimiter>,
    /// Per-email rate limiter for login attempts.
    pub login_limiter: Arc<AuthRateLimiter>,
    /// Per-email rate limiter for registration.
    pub register_limiter: Arc<AuthRateLimiter>,
    /// Per-IP rate limiter for unauthenticated API calls.
    pub ip_limiter: Arc<AuthRateLimiter>,
}

impl AppState {
    /// Builds application state from environment variables.
    ///
    /// Panics at startup if `AI_API_KEY` or `JWT_SECRET` are not set,
    /// or if `JWT_SECRET` is the insecure placeholder.
    pub async fn from_env(db: Option<PgPool>) -> Self {
        let client = ai::AiClient::from_env()
            .expect("AI_API_KEY must be set — see .env.example");

        let jwt_secret = std::env::var("JWT_SECRET")
            .expect("JWT_SECRET must be set — generate with: openssl rand -hex 32");

        if jwt_secret == "change-me-in-production" || jwt_secret.len() < 32 {
            panic!(
                "JWT_SECRET is insecure. Generate a proper secret with: openssl rand -hex 32"
            );
        }

        Self {
            conversation_engine: ConversationEngine::new(client),
            db,
            job_queue: Arc::new(JobQueue::new()),
            jwt_secret,
            rate_limiter: Arc::new(RateLimiter::hourly()),
            login_limiter: Arc::new(AuthRateLimiter::login()),
            register_limiter: Arc::new(AuthRateLimiter::register()),
            ip_limiter: Arc::new(AuthRateLimiter::unauthenticated()),
        }
    }
}

// ─── App builder ─────────────────────────────────────────────────────────────

pub async fn app() -> Router {
    let db = db::connect().await;
    let state = Arc::new(AppState::from_env(db).await);

    // Restrict CORS to known origins. Set ALLOWED_ORIGINS in production.
    let allowed_origins = std::env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    let cors = {
        use tower_http::cors::AllowOrigin;
        use axum::http::{HeaderValue, Method};

        let origins: Vec<HeaderValue> = allowed_origins
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();

        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
    };

    Router::new()
        .nest("/api", routes::router())
        .layer(cors)
        // Reject bodies larger than 256 KiB — prevents memory-exhaustion attacks.
        .layer(RequestBodyLimitLayer::new(256 * 1024))
        .with_state(state)
}
