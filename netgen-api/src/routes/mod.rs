use std::sync::Arc;

use axum::Router;

pub mod auth;
pub mod intent;
pub mod projects;
pub mod ws;

pub fn router() -> Router<Arc<crate::AppState>> {
    Router::new()
        .nest("/auth", auth::router())
        .nest("/intent", intent::router())
        .nest("/projects", projects::router())
        .nest("/projects", ws::router())
}
