//! Authenticated, bounded job submission and authoritative status polling.
mod error;
mod routes;
use crate::auth::{AuthError, JwksVerifier};
use axum::{
    Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use error::ApiError;
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;
const MAX_BODY: usize = 2 * 1024 * 1024;
#[derive(Clone)]
struct AppState {
    pool: PgPool,
    auth: Arc<JwksVerifier>,
    capacity: Arc<Semaphore>,
    timeout: Duration,
}
type WorkPermit = Arc<OwnedSemaphorePermit>;
#[derive(Clone, Copy)]
pub struct HttpLimits {
    pub max_in_flight: usize,
    pub request_timeout: Duration,
}
/// Build only with an authenticated key source and explicitly bounded capacity.
pub fn router(
    pool: PgPool,
    auth: Arc<JwksVerifier>,
    limits: HttpLimits,
) -> Result<Router, &'static str> {
    if !(1..=256).contains(&limits.max_in_flight)
        || limits.request_timeout < Duration::from_secs(1)
        || limits.request_timeout > Duration::from_secs(30)
    {
        return Err("invalid HTTP capacity/time limits");
    }
    let state = AppState {
        pool,
        auth,
        capacity: Arc::new(Semaphore::new(limits.max_in_flight)),
        timeout: limits.request_timeout,
    };
    let protected = Router::new()
        .route(
            "/v1/tenants/{tenant}/projects/{project}/jobs",
            post(routes::submit),
        )
        .route(
            "/v1/tenants/{tenant}/projects/{project}/jobs/{job}",
            get(routes::status),
        )
        .fallback(|| async {
            ApiError::new(StatusCode::NOT_FOUND, "not_found", "Route was not found")
        })
        .method_not_allowed_fallback(|| async {
            ApiError::new(
                StatusCode::METHOD_NOT_ALLOWED,
                "method_not_allowed",
                "Method is not supported",
            )
        })
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state);
    Ok(Router::new()
        .route("/health/live", get(|| async { StatusCode::NO_CONTENT }))
        .merge(protected))
}
fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, ApiError> {
    let mut values = headers.get_all(name).iter();
    let value = values.next().ok_or_else(ApiError::invalid)?;
    if values.next().is_some() {
        return Err(ApiError::invalid());
    }
    value.to_str().map_err(|_| ApiError::invalid())
}
async fn guard(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let request_id = Uuid::new_v4();
    let started = std::time::Instant::now();
    let response = async {
        let permit = state.capacity.clone().try_acquire_owned().map_err(|_| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "server_busy",
                "Request capacity is exhausted",
            )
        })?;
        let permit = Arc::new(permit);
        let auth = single_header(request.headers(), "authorization")
            .map_err(|_| ApiError::from(AuthError::InvalidToken))?;
        let (scheme, token) = auth.split_once(' ').ok_or(AuthError::InvalidToken)?;
        if !scheme.eq_ignore_ascii_case("bearer")
            || token.is_empty()
            || token.bytes().any(|b| b.is_ascii_whitespace())
        {
            return Err(AuthError::InvalidToken.into());
        }
        if token.len() > 16384 {
            return Err(AuthError::InvalidToken.into());
        }
        let token = token.to_owned();
        let result = tokio::time::timeout(state.timeout, async {
            let actor = state.auth.verify(&token).await?;
            request.extensions_mut().insert(actor);
            request.extensions_mut().insert(permit.clone());
            let response = next.run(request).await;
            drop(permit);
            Ok::<_, ApiError>(response)
        })
        .await;
        result.map_err(|_| {
            ApiError::new(
                StatusCode::GATEWAY_TIMEOUT,
                "request_timeout",
                "Request timed out; retry submissions with the same idempotency key",
            )
        })?
    }
    .await;
    let mut response = match response {
        Ok(r) => r,
        Err(e) => e.into_response(),
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("literal header"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().expect("literal header"),
    );
    response.headers_mut().insert(
        "x-request-id",
        request_id.to_string().parse().expect("UUID header"),
    );
    tracing::info!(event="http_request_complete",request_id=%request_id,status=response.status().as_u16(),elapsed_ms=u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
    response
}
#[cfg(all(test, feature = "postgres-tests"))]
mod database_tests;
#[cfg(test)]
mod tests;
