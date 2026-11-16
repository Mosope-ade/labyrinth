//! Signed bearer requests through the real router and restricted PostgreSQL role.
use super::*;
use crate::{MIGRATIONS, auth::test_support::*};
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
const RAW: &str = include_str!("../../../tests/fixtures/intents/enterprise.json");
const YAML: &str = include_str!("../../../tests/fixtures/catalog/blueprints.yaml");
struct Fixture {
    admin: PgPool,
    pool: PgPool,
    source: TestIssuer,
    auth: Arc<JwksVerifier>,
    tenant: Uuid,
    project: Uuid,
    actor: Uuid,
    hash: String,
}
impl Fixture {
    async fn new(quota: i32) -> Self {
        let url = std::env::var("LABYRINTH_TEST_DATABASE_URL")
            .expect("postgres-tests requires isolated database URL");
        let admin = PgPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .unwrap();
        MIGRATIONS.run(&admin).await.unwrap();
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(|c, _| {
                Box::pin(async move {
                    sqlx::query("SET ROLE labyrinth_api").execute(c).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        let source = TestIssuer::start().await;
        let auth = Arc::new(source.verifier().await.unwrap());
        let tenant = Uuid::new_v4();
        let project = Uuid::new_v4();
        let actor = Uuid::new_v4();
        sqlx::query("INSERT INTO labyrinth.users(id) VALUES($1)")
            .bind(actor)
            .execute(&admin)
            .await
            .unwrap();
        sqlx::query("INSERT INTO labyrinth.tenants(id,max_active_jobs) VALUES($1,$2)")
            .bind(tenant)
            .bind(quota)
            .execute(&admin)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO labyrinth.memberships(tenant_id,user_id,role) VALUES($1,$2,'owner')",
        )
        .bind(tenant)
        .bind(actor)
        .execute(&admin)
        .await
        .unwrap();
        sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,'HTTP test')")
            .bind(tenant)
            .bind(project)
            .execute(&admin)
            .await
            .unwrap();
        let c = labyrinth_engine::catalog::load_yaml(YAML).unwrap();
        let hash = c.version().sha256.clone();
        sqlx::query("INSERT INTO labyrinth.catalogs(sha256,semantic_version,yaml,admitted) VALUES($1,$2,$3,true) ON CONFLICT DO NOTHING").bind(&hash).bind(&c.version().semantic_version).bind(YAML).execute(&admin).await.unwrap();
        Self {
            admin,
            pool,
            source,
            auth,
            tenant,
            project,
            actor,
            hash,
        }
    }
    fn app(&self) -> Router {
        router(
            self.pool.clone(),
            self.auth.clone(),
            HttpLimits {
                max_in_flight: 4,
                request_timeout: Duration::from_secs(3),
            },
        )
        .unwrap()
    }
    fn path(&self) -> String {
        format!("/v1/tenants/{}/projects/{}/jobs", self.tenant, self.project)
    }
    fn body(&self) -> Value {
        json!({"intent":serde_json::from_str::<Value>(RAW).unwrap(),"catalog_sha256":self.hash,"expected_head_revision_id":null})
    }
    fn token(&self, actor: Uuid) -> String {
        self.source.signer.token(&claims(actor))
    }
    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Value,
        key: &str,
        actor: Uuid,
    ) -> (StatusCode, Value) {
        let response = self
            .app()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", format!("Bearer {}", self.token(actor)))
                    .header("content-type", "application/json")
                    .header("idempotency-key", key)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let code = response.status();
        let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
        (code, serde_json::from_slice(&bytes).unwrap())
    }
}
#[tokio::test]
async fn live_signed_submission_replay_and_restarted_router_status_use_one_durable_job() {
    let f = Fixture::new(2).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let app = f.app();
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let token = f.token(f.actor);
    let response = client
        .post(format!("{origin}{}", f.path()))
        .bearer_auth(&token)
        .header("idempotency-key", "live")
        .json(&f.body())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let location = response.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_owned();
    let first: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(first["state"], "queued");
    assert_eq!(first["replayed"], false);
    let (code, replay) = f.send("POST", &f.path(), f.body(), "live", f.actor).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(first["job_id"], replay["job_id"]);
    task.abort(); // New router uses persisted state, not the old HTTP process's memory.
    let (code, status) = f
        .send("GET", &location, Value::Null, "unused", f.actor)
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(status["job_id"], first["job_id"]);
    assert_eq!(status["state"], "queued");
    assert!(status["revision"].is_null());
    assert_eq!(status.as_object().unwrap().len(), 4);
    assert!(status["created_at"].as_str().unwrap().ends_with('Z'));
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM labyrinth.jobs WHERE tenant_id=$1")
        .bind(f.tenant)
        .fetch_one(&f.admin)
        .await
        .unwrap();
    assert_eq!(jobs, 1);
    let messages: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pgmq.q_labyrinth_generation_v1 WHERE message->>'tenant_id'=$1",
    )
    .bind(f.tenant.to_string())
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(messages, 1);
}
#[tokio::test]
async fn claims_cannot_spoof_membership_and_viewers_only_read_until_access_is_revoked() {
    let f = Fixture::new(4).await;
    let (_, job) = f.send("POST", &f.path(), f.body(), "job", f.actor).await;
    let path = format!("{}/{}", f.path(), job["job_id"].as_str().unwrap());
    let outsider = Uuid::new_v4();
    sqlx::query("INSERT INTO labyrinth.users(id) VALUES($1)")
        .bind(outsider)
        .execute(&f.admin)
        .await
        .unwrap();
    let mut fake_claims = claims(outsider);
    fake_claims["app_metadata"] = json!({"role":"admin","tenant_id":f.tenant});
    let response = f
        .app()
        .oneshot(
            Request::builder()
                .uri(&path)
                .header(
                    "authorization",
                    format!("Bearer {}", f.source.signer.token(&fake_claims)),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        f.send("POST", &f.path(), f.body(), "outsider", outsider)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE labyrinth.memberships SET role='viewer' WHERE tenant_id=$1")
        .bind(f.tenant)
        .execute(&f.admin)
        .await
        .unwrap();
    assert_eq!(
        f.send("GET", &path, Value::Null, "none", f.actor).await.0,
        StatusCode::OK
    );
    assert_eq!(
        f.send("POST", &f.path(), f.body(), "viewer", f.actor)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    sqlx::query("DELETE FROM labyrinth.memberships WHERE tenant_id=$1")
        .bind(f.tenant)
        .execute(&f.admin)
        .await
        .unwrap();
    assert_eq!(
        f.send("GET", &path, Value::Null, "none", f.actor).await.0,
        StatusCode::FORBIDDEN
    );
}
#[tokio::test]
async fn http_conflicts_clarification_and_quota_have_stable_errors_without_partial_admission() {
    let f = Fixture::new(1).await;
    let mut stale = f.body();
    stale["expected_head_revision_id"] = json!(Uuid::new_v4());
    let (code, error) = f.send("POST", &f.path(), stale, "stale", f.actor).await;
    assert_eq!(code, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "stale_head");
    let mut missing = f.body();
    missing["intent"]
        .as_object_mut()
        .unwrap()
        .remove("wan_services");
    let (code, error) = f.send("POST", &f.path(), missing, "clarify", f.actor).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error["error"]["code"], "needs_clarification");
    assert!(
        error["error"]["details"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == "/wan_services")
    );
    assert_eq!(
        f.send("POST", &f.path(), f.body(), "key", f.actor).await.0,
        StatusCode::ACCEPTED
    );
    let mut changed = f.body();
    changed["intent"]["name"] = json!("different");
    let (code, error) = f.send("POST", &f.path(), changed, "key", f.actor).await;
    assert_eq!(code, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "idempotency_conflict");
    let (code, error) = f.send("POST", &f.path(), f.body(), "other", f.actor).await;
    assert_eq!(code, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(error["error"]["code"], "quota_exceeded");
}
#[tokio::test]
async fn status_scopes_project_and_job_and_rechecks_suspension_and_storage_failure() {
    let f = Fixture::new(3).await;
    let (_, job) = f.send("POST", &f.path(), f.body(), "key", f.actor).await;
    let path = format!("{}/{}", f.path(), job["job_id"].as_str().unwrap());
    assert_eq!(
        f.send(
            "GET",
            &format!("{}/{}", f.path(), Uuid::new_v4()),
            Value::Null,
            "none",
            f.actor
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,'other')")
        .bind(f.tenant)
        .bind(other)
        .execute(&f.admin)
        .await
        .unwrap();
    let other_path = path.replace(&f.project.to_string(), &other.to_string());
    assert_eq!(
        f.send("GET", &other_path, Value::Null, "none", f.actor)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    sqlx::query("UPDATE labyrinth.users SET suspended=true WHERE id=$1")
        .bind(f.actor)
        .execute(&f.admin)
        .await
        .unwrap();
    assert_eq!(
        f.send("GET", &path, Value::Null, "none", f.actor).await.0,
        StatusCode::FORBIDDEN
    );
    f.pool.close().await;
    let (code, error) = f.send("GET", &path, Value::Null, "none", f.actor).await;
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error["error"]["code"], "database_unavailable");
    assert!(!error.to_string().contains("SQL"));
}
#[tokio::test]
async fn public_read_functions_and_internal_authorizer_are_not_client_callable() {
    let f = Fixture::new(2).await;
    for name in ["job_status", "admission_catalog", "require_project_access"] {
        let public_grants:i64=sqlx::query_scalar("SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace CROSS JOIN LATERAL aclexplode(coalesce(p.proacl,acldefault('f',p.proowner))) a WHERE n.nspname='labyrinth' AND p.proname=$1 AND a.grantee=0 AND a.privilege_type='EXECUTE'").bind(name).fetch_one(&f.admin).await.unwrap();
        assert_eq!(public_grants, 0);
    }
    let error = sqlx::query("SELECT labyrinth.require_project_access($1,$2,$3,false)")
        .bind(f.actor)
        .bind(f.tenant)
        .bind(f.project)
        .execute(&f.pool)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("42501")
    );
}

#[tokio::test]
async fn status_exposes_published_identity_only_when_success_has_revision_metadata() {
    let f = Fixture::new(2).await;
    let (_, job) = f.send("POST", &f.path(), f.body(), "key", f.actor).await;
    let job_id: Uuid = job["job_id"].as_str().unwrap().parse().unwrap();
    let path = format!("{}/{}", f.path(), job_id);
    sqlx::query("UPDATE labyrinth.jobs SET state='running' WHERE tenant_id=$1")
        .bind(f.tenant)
        .execute(&f.admin)
        .await
        .unwrap();
    assert_eq!(
        f.send("GET", &path, Value::Null, "none", f.actor).await.1["state"],
        "running"
    );
    sqlx::query("UPDATE labyrinth.jobs SET state='succeeded' WHERE tenant_id=$1")
        .bind(f.tenant)
        .execute(&f.admin)
        .await
        .unwrap();
    // Deliberately inconsistent maintenance state is never presented as a result.
    assert_eq!(
        f.send("GET", &path, Value::Null, "none", f.actor).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let revision = Uuid::new_v4();
    let digest = "a".repeat(64);
    sqlx::query("INSERT INTO labyrinth.revisions(tenant_id,project_id,id,job_id,manifest_sha256,catalog_sha256) VALUES($1,$2,$3,$4,$5,$6)").bind(f.tenant).bind(f.project).bind(revision).bind(job_id).bind(&digest).bind(&f.hash).execute(&f.admin).await.unwrap();
    let (code, status) = f.send("GET", &path, Value::Null, "none", f.actor).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(status["revision"]["revision_id"], revision.to_string());
    assert_eq!(status["revision"]["manifest_sha256"], digest);
}
#[tokio::test]
async fn invalid_intent_duplicate_fields_and_expired_auth_keys_do_not_create_jobs() {
    let f = Fixture::new(2).await;
    let body = f.body().to_string().replacen(
        "\"name\":\"Enterprise baseline\"",
        "\"name\":\"one\",\"name\":\"two\"",
        1,
    );
    assert!(body.contains("\"name\":\"two\""));
    let response = f
        .app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(f.path())
                .header("authorization", format!("Bearer {}", f.token(f.actor)))
                .header("content-type", "application/json")
                .header("idempotency-key", "duplicate-fields")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let mut invalid = f.body();
    invalid["intent"]["sites"][0]["zones"][0]["endpoint_count"] = json!(0);
    let (code, error) = f.send("POST", &f.path(), invalid, "invalid", f.actor).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(error["error"]["details"]["issues"].is_array());
    f.source.fail();
    f.auth.expire_for_test().await;
    let (code, error) = f
        .send("POST", &f.path(), f.body(), "keys-down", f.actor)
        .await;
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error["error"]["code"], "authentication_unavailable");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM labyrinth.jobs WHERE tenant_id=$1")
        .bind(f.tenant)
        .fetch_one(&f.admin)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn cancellation_is_authenticated_scoped_idempotent_and_preserves_terminal_history() {
    let f = Fixture::new(2).await;
    let (_, job) = f.send("POST", &f.path(), f.body(), "cancel", f.actor).await;
    let id = job["job_id"].as_str().unwrap();
    let path = format!("{}/{id}/cancel", f.path());
    let (code, _) = f.send("POST", &path, json!({}), "", Uuid::new_v4()).await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    sqlx::query("UPDATE labyrinth.memberships SET role='viewer' WHERE tenant_id=$1 AND user_id=$2")
        .bind(f.tenant)
        .bind(f.actor)
        .execute(&f.admin)
        .await
        .unwrap();
    let (code, _) = f.send("POST", &path, json!({}), "", f.actor).await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    sqlx::query("UPDATE labyrinth.memberships SET role='editor' WHERE tenant_id=$1 AND user_id=$2")
        .bind(f.tenant)
        .bind(f.actor)
        .execute(&f.admin)
        .await
        .unwrap();
    for _ in 0..2 {
        let (code, body) = f.send("POST", &path, json!({}), "", f.actor).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["state"], "cancelled");
    }
    let (code, body) = f
        .send("GET", &format!("{}/{id}", f.path()), json!({}), "", f.actor)
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["state"], "cancelled");
}

#[tokio::test]
async fn downloads_only_release_authorized_published_hash_verified_bytes() {
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicBool, Ordering};
    let f = Fixture::new(2).await;
    let (_, job) = f
        .send("POST", &f.path(), f.body(), "download", f.actor)
        .await;
    let job: Uuid = serde_json::from_value(job["job_id"].clone()).unwrap();
    let revision = Uuid::new_v4();
    let object_key = format!("attempts/{revision}/manifest.json");
    let bytes = b"{\"verified\":true}\n";
    let digest = format!("{:x}", Sha256::digest(bytes));
    sqlx::query("UPDATE labyrinth.jobs SET state='running' WHERE tenant_id=$1 AND id=$2")
        .bind(f.tenant)
        .bind(job)
        .execute(&f.admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO labyrinth.revisions(tenant_id,project_id,id,job_id,manifest_sha256,catalog_sha256) VALUES($1,$2,$3,$4,$5,$6)").bind(f.tenant).bind(f.project).bind(revision).bind(job).bind(&digest).bind(&f.hash).execute(&f.admin).await.unwrap();
    sqlx::query("INSERT INTO labyrinth.artifacts(tenant_id,project_id,revision_id,name,object_key,sha256,size_bytes) VALUES($1,$2,$3,'manifest.json',$6,$4,$5)").bind(f.tenant).bind(f.project).bind(revision).bind(&digest).bind(bytes.len() as i64).bind(&object_key).execute(&f.admin).await.unwrap();
    sqlx::query("UPDATE labyrinth.jobs SET state='succeeded' WHERE tenant_id=$1 AND id=$2")
        .bind(f.tenant)
        .bind(job)
        .execute(&f.admin)
        .await
        .unwrap();
    let corrupt = Arc::new(AtomicBool::new(false));
    let revoke = Arc::new(AtomicBool::new(false));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}/", listener.local_addr().unwrap());
    let admin = f.admin.clone();
    let actor = f.actor;
    let tenant = f.tenant;
    let corrupt_flag = corrupt.clone();
    let revoke_flag = revoke.clone();
    let storage = Router::new().route(
        &format!("/storage/v1/object/authenticated/private/{object_key}"),
        get(move |headers: HeaderMap| {
            let admin = admin.clone();
            let corrupt = corrupt_flag.clone();
            let revoke = revoke_flag.clone();
            async move {
                assert_eq!(headers["authorization"], "Bearer test-only");
                if revoke.load(Ordering::SeqCst) {
                    sqlx::query(
                        "DELETE FROM labyrinth.memberships WHERE tenant_id=$1 AND user_id=$2",
                    )
                    .bind(tenant)
                    .bind(actor)
                    .execute(&admin)
                    .await
                    .unwrap();
                }
                if corrupt.load(Ordering::SeqCst) {
                    let mut invalid = bytes.to_vec();
                    invalid[0] = b'!';
                    invalid
                } else {
                    bytes.to_vec()
                }
            }
        }),
    );
    let task = tokio::spawn(async move { axum::serve(listener, storage).await.unwrap() });
    let app = router_with_artifacts(
        f.pool.clone(),
        f.auth.clone(),
        HttpLimits {
            max_in_flight: 1,
            request_timeout: Duration::from_secs(3),
        },
        Some(Arc::new(crate::artifacts::ArtifactReader::for_test(
            &origin,
        ))),
    )
    .unwrap();
    let path = format!(
        "/v1/tenants/{}/projects/{}/revisions/{revision}/artifacts/manifest.json",
        f.tenant, f.project
    );
    let request = |actor| {
        Request::builder()
            .uri(&path)
            .header(
                "authorization",
                format!("Bearer {}", f.source.signer.token(&claims(actor))),
            )
            .body(Body::empty())
            .unwrap()
    };
    let response = app.clone().oneshot(request(f.actor)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        app.clone()
            .oneshot(request(f.actor))
            .await
            .unwrap()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "download body must retain admission capacity until consumed or dropped"
    );
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(response.headers().get("location").is_none());
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap().as_ref(),
        bytes
    );
    let denied = app.clone().oneshot(request(Uuid::new_v4())).await.unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    corrupt.store(true, Ordering::SeqCst);
    assert_eq!(
        app.clone()
            .oneshot(request(f.actor))
            .await
            .unwrap()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    corrupt.store(false, Ordering::SeqCst);
    revoke.store(true, Ordering::SeqCst);
    assert_eq!(
        app.oneshot(request(f.actor)).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    task.abort();
}

#[tokio::test]
async fn project_discovery_is_bounded_stable_and_tenant_scoped() {
    let f = Fixture::new(2).await;
    for name in ["second", "third"] {
        sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,$3)")
            .bind(f.tenant)
            .bind(Uuid::new_v4())
            .bind(name)
            .execute(&f.admin)
            .await
            .unwrap();
    }
    let path = format!("/v1/tenants/{}/projects", f.tenant);
    let (code, first) = f
        .send("GET", &format!("{path}?limit=2"), json!({}), "", f.actor)
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    let cursor = first["next_after"].as_str().unwrap();
    let (code, second) = f
        .send(
            "GET",
            &format!("{path}?limit=2&after={cursor}"),
            json!({}),
            "",
            f.actor,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert!(second["next_after"].is_null());
    let mut ids: Vec<_> = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second["items"].as_array().unwrap())
        .map(|v| v["project_id"].as_str().unwrap())
        .collect();
    let original = ids.clone();
    ids.sort();
    ids.dedup();
    assert_eq!(ids, original);
    for query in [
        "limit=0",
        "limit=101",
        "limit=-1",
        "limit=1&limit=2",
        "after=invalid",
        "unknown=x",
    ] {
        let (code, _) = f
            .send("GET", &format!("{path}?{query}"), json!({}), "", f.actor)
            .await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }
    let (code, _) = f.send("GET", &path, json!({}), "", Uuid::new_v4()).await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    sqlx::query("UPDATE labyrinth.memberships SET role='viewer' WHERE tenant_id=$1 AND user_id=$2")
        .bind(f.tenant)
        .bind(f.actor)
        .execute(&f.admin)
        .await
        .unwrap();
    let (code, _) = f.send("GET", &path, json!({}), "", f.actor).await;
    assert_eq!(code, StatusCode::OK);
    sqlx::query("UPDATE labyrinth.tenants SET suspended=true WHERE id=$1")
        .bind(f.tenant)
        .execute(&f.admin)
        .await
        .unwrap();
    let (code, _) = f.send("GET", &path, json!({}), "", f.actor).await;
    assert_eq!(code, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn revision_discovery_exposes_published_metadata_without_private_keys() {
    let f = Fixture::new(4).await;
    let mut revisions = vec![];
    for i in 0..3 {
        let (_, job) = f
            .send(
                "POST",
                &f.path(),
                f.body(),
                &format!("revision-{i}"),
                f.actor,
            )
            .await;
        let job: Uuid = serde_json::from_value(job["job_id"].clone()).unwrap();
        let revision = Uuid::new_v4();
        sqlx::query("UPDATE labyrinth.jobs SET state='running' WHERE tenant_id=$1 AND id=$2")
            .bind(f.tenant)
            .bind(job)
            .execute(&f.admin)
            .await
            .unwrap();
        sqlx::query("INSERT INTO labyrinth.revisions(tenant_id,project_id,id,job_id,manifest_sha256,catalog_sha256) VALUES($1,$2,$3,$4,$5,$6)").bind(f.tenant).bind(f.project).bind(revision).bind(job).bind("a".repeat(64)).bind(&f.hash).execute(&f.admin).await.unwrap();
        sqlx::query("INSERT INTO labyrinth.artifacts(tenant_id,project_id,revision_id,name,object_key,sha256,size_bytes) VALUES($1,$2,$3,'manifest.json',$4,$5,1)").bind(f.tenant).bind(f.project).bind(revision).bind(format!("attempts/{revision}/manifest.json")).bind("a".repeat(64)).execute(&f.admin).await.unwrap();
        if i < 2 {
            sqlx::query("UPDATE labyrinth.jobs SET state='succeeded' WHERE tenant_id=$1 AND id=$2")
                .bind(f.tenant)
                .bind(job)
                .execute(&f.admin)
                .await
                .unwrap();
        }
        revisions.push(revision);
    }
    let path = format!("/v1/tenants/{}/projects/{}/revisions", f.tenant, f.project);
    let (code, first) = f
        .send("GET", &format!("{path}?limit=1"), json!({}), "", f.actor)
        .await;
    assert_eq!(code, StatusCode::OK);
    let cursor = first["next_after"].as_str().unwrap();
    let (code, second) = f
        .send(
            "GET",
            &format!("{path}?limit=1&after={cursor}"),
            json!({}),
            "",
            f.actor,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert!(second["next_after"].is_null());
    assert_ne!(
        first["items"][0]["revision_id"],
        second["items"][0]["revision_id"]
    );
    let (code, detail) = f
        .send(
            "GET",
            &format!("{path}/{}", revisions[0]),
            json!({}),
            "",
            f.actor,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(detail["artifacts"][0]["name"], "manifest.json");
    assert!(!detail.to_string().contains("object_key"));
    assert!(!detail.to_string().contains("requirements"));
    let (code, _) = f
        .send(
            "GET",
            &format!("{path}/{}", revisions[2]),
            json!({}),
            "",
            f.actor,
        )
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let (code, _) = f.send("GET", &path, json!({}), "", Uuid::new_v4()).await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    let wrong = format!(
        "/v1/tenants/{}/projects/{}/revisions/{}",
        f.tenant,
        Uuid::new_v4(),
        revisions[0]
    );
    let (code, _) = f.send("GET", &wrong, json!({}), "", f.actor).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let helper:bool=sqlx::query_scalar("SELECT has_function_privilege('labyrinth_api','labyrinth.require_tenant_access(uuid,uuid)','EXECUTE')").fetch_one(&f.admin).await.unwrap();
    assert!(!helper);
}
