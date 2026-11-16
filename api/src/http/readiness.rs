//! Dependency readiness without authentication tokens or operational disclosures.
use super::AppState;
use axum::{
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use std::time::Duration;
pub(super) async fn ready(State(state): State<AppState>) -> Response {
    let result = async {
        let _permit = state.capacity.clone().try_acquire_owned().map_err(|_| ())?;
        let correct:bool=sqlx::query_scalar("SELECT current_user='labyrinth_api' AND to_regprocedure('labyrinth.onboard_personal(uuid,integer,integer)') IS NOT NULL AND to_regprocedure('labyrinth.job_attempts(uuid,uuid,uuid,uuid)') IS NOT NULL AND to_regprocedure('labyrinth.admin_cancel_job(uuid,uuid,uuid,uuid,text)') IS NOT NULL AND to_regprocedure('labyrinth.prune_admin_audit(integer)') IS NOT NULL AND to_regprocedure('labyrinth.attempt_evidence(uuid,uuid,uuid,uuid,uuid)') IS NOT NULL")
            .fetch_one(&state.pool).await.map_err(|_|())?;
        if !correct {
            return Err(());
        }
        state.auth.ready().await.map_err(|_| ())
    };
    let code = if matches!(
        tokio::time::timeout(state.timeout.min(Duration::from_secs(10)), result).await,
        Ok(Ok(()))
    ) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    ([(header::CACHE_CONTROL, "no-store")], code).into_response()
}
