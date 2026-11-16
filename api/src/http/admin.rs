//! Operator-managed authority and redacted operational metadata; no tenant bypass.
use super::{
    ApiError, AppState, WorkPermit,
    discovery::{PageQuery, page},
    single_header,
};
use crate::{auth::AuthenticatedActor, jobs::AdmissionError};
use axum::{
    Json,
    body::Bytes,
    extract::{
        Extension, FromRequestParts, Path, Query, State,
        rejection::{BytesRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;
pub(super) struct AdminActor(Uuid);
impl FromRequestParts<AppState> for AdminActor {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let actor = parts
            .extensions
            .get::<AuthenticatedActor>()
            .ok_or_else(forbidden)?
            .id();
        let allowed: bool = sqlx::query_scalar("SELECT labyrinth.admin_access($1)")
            .bind(actor)
            .fetch_one(&state.pool)
            .await
            .map_err(AdmissionError::from)?;
        if !allowed {
            sqlx::query("SELECT labyrinth.admin_denied($1)")
                .bind(actor)
                .execute(&state.pool)
                .await
                .map_err(AdmissionError::from)?;
            return Err(forbidden());
        }
        Ok(Self(actor))
    }
}
fn forbidden() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "forbidden",
        "Administrator permission is required",
    )
}
fn result(value: Value) -> Result<Response, ApiError> {
    if let Some(code) = value["error"].as_str() {
        return Err(match code {
            "forbidden" => forbidden(),
            "operator_required" => ApiError::new(
                StatusCode::FORBIDDEN,
                "operator_required",
                "Administrator accounts require operator maintenance",
            ),
            "not_found" => ApiError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                "Operational target was not found",
            ),
            "stale_state" => ApiError::new(
                StatusCode::CONFLICT,
                "stale_state",
                "Operational state changed; review the current value",
            ),
            "catalog_version_conflict" => ApiError::new(
                StatusCode::CONFLICT,
                "catalog_version_conflict",
                "Catalog version already identifies another hash",
            ),
            _ => ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Operation could not be processed",
            ),
        });
    }
    Ok(Json(value).into_response())
}
fn decode<T: serde::de::DeserializeOwned>(
    headers: &HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<T, ApiError> {
    if !single_header(headers, "content-type")?
        .split(';')
        .next()
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Content-Type must be application/json",
        ));
    }
    let body = body.map_err(|_| {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Request exceeds 2 MiB",
        )
    })?;
    serde_json::from_slice(&body).map_err(|_| ApiError::invalid())
}
pub(super) async fn overview(
    State(state): State<AppState>,
    AdminActor(actor): AdminActor,
) -> Result<Response, ApiError> {
    let value: Value = sqlx::query_scalar("SELECT labyrinth.admin_overview($1)")
        .bind(actor)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    result(value)
}
async fn entities(
    state: AppState,
    actor: Uuid,
    kind: &str,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::invalid())?;
    let limit = query.limit()?;
    let value: Value = sqlx::query_scalar("SELECT labyrinth.admin_entities($1,$2,$3,$4)")
        .bind(actor)
        .bind(kind)
        .bind(query.after)
        .bind(limit as i32)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    page(value, limit, "id")
}
pub(super) async fn users(
    State(state): State<AppState>,
    AdminActor(actor): AdminActor,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    entities(state, actor, "user", query).await
}
pub(super) async fn tenants(
    State(state): State<AppState>,
    AdminActor(actor): AdminActor,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Result<Response, ApiError> {
    entities(state, actor, "tenant", query).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AuditPage {
    limit: Option<usize>,
    before: Option<i64>,
}
pub(super) async fn audit(
    State(state): State<AppState>,
    AdminActor(actor): AdminActor,
    query: Result<Query<AuditPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::invalid())?;
    let limit = query.limit.unwrap_or(25);
    if !(1..=100).contains(&limit) || query.before.is_some_and(|n| n <= 0) {
        return Err(ApiError::invalid());
    }
    let value: Value = sqlx::query_scalar("SELECT labyrinth.admin_audit($1,$2,$3)")
        .bind(actor)
        .bind(query.before)
        .bind(limit as i32)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    let mut items = value.as_array().ok_or_else(ApiError::invalid)?.clone();
    let more = items.len() > limit;
    items.truncate(limit);
    let next = if more {
        items.last().and_then(|v| v.get("id")).cloned()
    } else {
        None
    };
    Ok(Json(json!({"items":items,"next_before":next})).into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Draft {
    yaml: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Promote {
    yaml: String,
    expected_sha256: String,
    confirm_sha256: String,
}
fn hash_valid(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
async fn note(
    state: &AppState,
    actor: Uuid,
    action: &str,
    hash: Option<&str>,
    valid: bool,
) -> Result<(), ApiError> {
    let allowed: bool = sqlx::query_scalar("SELECT labyrinth.admin_catalog_note($1,$2,$3,$4)")
        .bind(actor)
        .bind(action)
        .bind(hash)
        .bind(valid)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    if allowed { Ok(()) } else { Err(forbidden()) }
}
async fn catalog(
    yaml: String,
    permit: WorkPermit,
) -> Result<labyrinth_engine::catalog::CatalogSnapshot, ApiError> {
    if yaml.len() > 1024 * 1024 {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Catalog exceeds 1 MiB",
        ));
    }
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        labyrinth_engine::catalog::load_yaml(&yaml)
    })
    .await
    .map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Catalog validation could not complete",
        )
    })?
    .map_err(|_| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_catalog",
            "Catalog failed strict schema or semantic validation",
        )
    })
}
pub(super) async fn validate_catalog(
    State(state): State<AppState>,
    AdminActor(actor): AdminActor,
    Extension(permit): Extension<WorkPermit>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let draft: Draft = decode(&headers, body)?;
    let checked = catalog(draft.yaml, permit).await;
    note(
        &state,
        actor,
        "catalog_validate",
        checked.as_ref().ok().map(|c| c.version().sha256.as_str()),
        checked.is_ok(),
    )
    .await?;
    let checked = checked?;
    Ok(
        Json(json!({"catalog_version":checked.version(),"archetypes":checked.archetypes().len()}))
            .into_response(),
    )
}
pub(super) async fn promote(
    State(state): State<AppState>,
    AdminActor(actor): AdminActor,
    Extension(permit): Extension<WorkPermit>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let draft: Promote = decode(&headers, body)?;
    if !hash_valid(&draft.expected_sha256) || draft.confirm_sha256 != draft.expected_sha256 {
        return Err(ApiError::invalid());
    }
    let checked = catalog(draft.yaml.clone(), permit).await;
    if !checked
        .as_ref()
        .is_ok_and(|c| c.version().sha256 == draft.expected_sha256)
    {
        note(
            &state,
            actor,
            "catalog_promote",
            Some(&draft.expected_sha256),
            false,
        )
        .await?;
    }
    let checked = checked?;
    if checked.version().sha256 != draft.expected_sha256 {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "catalog_hash_mismatch",
            "Reviewed catalog hash does not match the supplied draft",
        ));
    }
    let value: Value = sqlx::query_scalar("SELECT labyrinth.admin_promote_catalog($1,$2,$3,$4)")
        .bind(actor)
        .bind(&checked.version().sha256)
        .bind(&checked.version().semantic_version)
        .bind(&draft.yaml)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    result(value)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdmissionChange {
    expected_admitted: bool,
    admitted: bool,
    confirm_sha256: String,
}
pub(super) async fn catalog_state(
    State(state): State<AppState>,
    AdminActor(actor): AdminActor,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let Path(hash) = path.map_err(|_| ApiError::invalid())?;
    let change: AdmissionChange = decode(&headers, body)?;
    if !hash_valid(&hash) || change.confirm_sha256 != hash {
        return Err(ApiError::invalid());
    }
    let value: Value = sqlx::query_scalar("SELECT labyrinth.admin_catalog_state($1,$2,$3,$4)")
        .bind(actor)
        .bind(hash)
        .bind(change.expected_admitted)
        .bind(change.admitted)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    result(value)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Suspension {
    expected_suspended: bool,
    suspended: bool,
    confirm_target: Uuid,
}
async fn suspend(
    state: AppState,
    actor: Uuid,
    kind: &str,
    target: Uuid,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let change: Suspension = decode(&headers, body)?;
    if change.confirm_target != target {
        return Err(ApiError::invalid());
    }
    let value: Value = sqlx::query_scalar("SELECT labyrinth.admin_suspend($1,$2,$3,$4,$5)")
        .bind(actor)
        .bind(kind)
        .bind(target)
        .bind(change.expected_suspended)
        .bind(change.suspended)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    result(value)
}
pub(super) async fn user_suspend(
    State(state): State<AppState>,
    AdminActor(actor): AdminActor,
    path: Result<Path<Uuid>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let Path(target) = path.map_err(|_| ApiError::invalid())?;
    suspend(state, actor, "user", target, headers, body).await
}
pub(super) async fn tenant_suspend(
    State(state): State<AppState>,
    AdminActor(actor): AdminActor,
    path: Result<Path<Uuid>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let Path(target) = path.map_err(|_| ApiError::invalid())?;
    suspend(state, actor, "tenant", target, headers, body).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelJob {
    expected_state: String,
    confirm_job: Uuid,
}
pub(super) async fn job(
    State(state): State<AppState>,
    AdminActor(actor): AdminActor,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, project, job)) = path.map_err(|_| ApiError::invalid())?;
    let value: Value = sqlx::query_scalar("SELECT labyrinth.admin_job($1,$2,$3,$4)")
        .bind(actor)
        .bind(tenant)
        .bind(project)
        .bind(job)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    result(value)
}
pub(super) async fn cancel_job(
    State(state): State<AppState>,
    AdminActor(actor): AdminActor,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, project, job)) = path.map_err(|_| ApiError::invalid())?;
    let change: CancelJob = decode(&headers, body)?;
    if change.confirm_job != job
        || ![
            "queued",
            "running",
            "retry_wait",
            "cancel_requested",
            "cancelled",
            "failed",
            "succeeded",
        ]
        .contains(&change.expected_state.as_str())
    {
        return Err(ApiError::invalid());
    }
    let value: Value = sqlx::query_scalar("SELECT labyrinth.admin_cancel_job($1,$2,$3,$4,$5)")
        .bind(actor)
        .bind(tenant)
        .bind(project)
        .bind(job)
        .bind(change.expected_state)
        .fetch_one(&state.pool)
        .await
        .map_err(AdmissionError::from)?;
    result(value)
}
