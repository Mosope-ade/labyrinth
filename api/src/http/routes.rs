use super::{ApiError, AppState, WorkPermit, single_header};
use crate::{
    auth::AuthenticatedActor,
    jobs::{self, AdmissionContext, PreparedAdmission},
};
use axum::{
    Json,
    body::Bytes,
    extract::{
        Extension, Path, State,
        rejection::{BytesRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitRequest {
    intent: Box<RawValue>,
    #[serde(default)]
    options: labyrinth_engine::artifacts::GenerationOptions,
    catalog_sha256: String,
    #[serde(deserialize_with = "required_head")]
    expected_head_revision_id: Option<Uuid>,
}
fn required_head<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Uuid>, D::Error> {
    Option::deserialize(d)
}
pub(super) async fn submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    Extension(permit): Extension<WorkPermit>,
    path: Result<Path<(Uuid, Uuid)>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, project)) = path.map_err(|_| ApiError::invalid())?;
    let body = body.map_err(|e| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "Request body exceeds 2 MiB",
            )
        } else {
            ApiError::invalid()
        }
    })?;
    if !single_header(&headers, "content-type")
        .map_err(|_| {
            ApiError::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                "Content-Type must be application/json",
            )
        })?
        .split(';')
        .next()
        .is_some_and(|s| s.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Content-Type must be application/json",
        ));
    }
    let key = single_header(&headers, "idempotency-key")?.to_owned();
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
    {
        return Err(ApiError::invalid());
    }
    let work = permit.clone();
    let request: SubmitRequest = tokio::task::spawn_blocking(move || {
        let _permit = work;
        serde_json::from_slice(&body)
    })
    .await
    .map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Request could not be processed",
        )
    })?
    .map_err(|_| ApiError::invalid())?;
    if request.catalog_sha256.len() != 64
        || !request
            .catalog_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ApiError::invalid());
    }
    let context = AdmissionContext {
        actor_id: actor.id(),
        tenant_id: tenant,
        project_id: project,
        expected_head: request.expected_head_revision_id,
    };
    let catalog =
        jobs::catalog_for_admission(&state.pool, &context, &request.catalog_sha256).await?;
    let prepared = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        PreparedAdmission::new(
            context,
            &key,
            request.intent.get(),
            &request.options,
            &catalog,
        )
    })
    .await
    .map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Request could not be processed",
        )
    })??;
    let result = jobs::admit(&state.pool, &prepared).await?;
    let location = format!(
        "/v1/tenants/{tenant}/projects/{project}/jobs/{}",
        result.job_id
    );
    let code = if result.replayed {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok((code, [(header::LOCATION, location)], Json(result)).into_response())
}
pub(super) async fn status(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, project, job)) = path.map_err(|_| ApiError::invalid())?;
    Ok(Json(jobs::status(&state.pool, actor.id(), tenant, project, job).await?).into_response())
}

pub(super) async fn cancel(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    path: Result<Path<(Uuid, Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let Path((tenant, project, job)) = path.map_err(|_| ApiError::invalid())?;
    let status: crate::jobs::JobState =
        sqlx::query_scalar("SELECT labyrinth.cancel_job($1,$2,$3,$4)")
            .bind(actor.id())
            .bind(tenant)
            .bind(project)
            .bind(job)
            .fetch_one(&state.pool)
            .await
            .map_err(crate::jobs::AdmissionError::from)?;
    Ok(Json(serde_json::json!({"job_id":job,"state":status})).into_response())
}

pub(super) async fn download(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedActor>,
    Extension(permit): Extension<WorkPermit>,
    path: Result<Path<(Uuid, Uuid, Uuid, String)>, PathRejection>,
) -> Result<Response, ApiError> {
    use sha2::{Digest, Sha256};
    use sqlx::Row;
    let Path((tenant, project, revision, name)) = path.map_err(|_| ApiError::invalid())?;
    let lookup = || {
        sqlx::query("SELECT * FROM labyrinth.download_artifact($1,$2,$3,$4,$5)")
            .bind(actor.id())
            .bind(tenant)
            .bind(project)
            .bind(revision)
            .bind(&name)
    };
    let row = lookup()
        .fetch_optional(&state.pool)
        .await
        .map_err(crate::jobs::AdmissionError::from)?
        .ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, "not_found", "Artifact was not found")
        })?;
    let unavailable = || {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "artifact_unavailable",
            "Artifact is temporarily unavailable",
        )
    };
    let reader = state.artifacts.as_ref().ok_or_else(unavailable)?;
    let key: String = row.get("object_key");
    let hash: String = row.get("sha256");
    let size: i64 = row.get("size_bytes");
    let bytes = reader
        .read(&key, size as u64)
        .await
        .map_err(|_| unavailable())?;
    if format!("{:x}", Sha256::digest(&bytes)) != hash {
        return Err(unavailable());
    }
    // Recheck current access after network I/O, before releasing sensitive bytes.
    lookup()
        .fetch_optional(&state.pool)
        .await
        .map_err(crate::jobs::AdmissionError::from)?
        .ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, "not_found", "Artifact was not found")
        })?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/json".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
            (header::ETAG, format!("\"{hash}\"")),
        ],
        axum::body::Body::from_stream(DownloadBody {
            bytes: bytes.into(),
            _permit: permit,
        }),
    )
        .into_response())
}

// Retain request capacity through body consumption, including slow clients.
struct DownloadBody {
    bytes: axum::body::Bytes,
    _permit: WorkPermit,
}
impl tokio_stream::Stream for DownloadBody {
    type Item = Result<axum::body::Bytes, std::convert::Infallible>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        if self.bytes.is_empty() {
            std::task::Poll::Ready(None)
        } else {
            let size = self.bytes.len().min(64 * 1024);
            std::task::Poll::Ready(Some(Ok(self.bytes.split_to(size))))
        }
    }
}
