#![cfg(feature = "postgres-tests")]
use labyrinth_api::{MIGRATIONS, jobs::*};
use labyrinth_engine::catalog;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::Arc;
use uuid::Uuid;
const RAW: &str = include_str!("../../tests/fixtures/intents/enterprise.json");
const YAML: &str = include_str!("../../tests/fixtures/catalog/blueprints.yaml");
struct Fixture {
    admin: PgPool,
    api: PgPool,
    context: AdmissionContext,
}
impl Fixture {
    async fn new(quota: i32) -> Self {
        let url = std::env::var("LABYRINTH_TEST_DATABASE_URL").expect("postgres-tests requires an isolated LABYRINTH_TEST_DATABASE_URL; tests never silently skip");
        let admin = PgPoolOptions::new()
            .max_connections(8)
            .connect(&url)
            .await
            .unwrap();
        MIGRATIONS.run(&admin).await.unwrap();
        // Running twice proves checksum-compatible repeat startup, not SQL replay.
        MIGRATIONS.run(&admin).await.unwrap();
        let api = PgPoolOptions::new()
            .max_connections(8)
            .after_connect(|c, _| {
                Box::pin(async move {
                    sqlx::query("SET ROLE labyrinth_api").execute(c).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        let context = AdmissionContext {
            actor_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            expected_head: None,
        };
        sqlx::query("INSERT INTO labyrinth.users(id) VALUES($1)")
            .bind(context.actor_id)
            .execute(&admin)
            .await
            .unwrap();
        sqlx::query("INSERT INTO labyrinth.tenants(id,max_active_jobs) VALUES($1,$2)")
            .bind(context.tenant_id)
            .bind(quota)
            .execute(&admin)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO labyrinth.memberships(tenant_id,user_id,role) VALUES($1,$2,'owner')",
        )
        .bind(context.tenant_id)
        .bind(context.actor_id)
        .execute(&admin)
        .await
        .unwrap();
        sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,'test')")
            .bind(context.tenant_id)
            .bind(context.project_id)
            .execute(&admin)
            .await
            .unwrap();
        let c = catalog::load_yaml(YAML).unwrap();
        sqlx::query("INSERT INTO labyrinth.catalogs(sha256,semantic_version,yaml,admitted) VALUES($1,$2,$3,true) ON CONFLICT DO NOTHING")
            .bind(&c.version().sha256).bind(&c.version().semantic_version).bind(YAML).execute(&admin).await.unwrap();
        Self {
            admin,
            api,
            context,
        }
    }
    fn prepare(&self, key: &str) -> PreparedAdmission {
        self.prepare_raw(self.context.clone(), key, RAW)
    }
    fn prepare_raw(&self, ctx: AdmissionContext, key: &str, raw: &str) -> PreparedAdmission {
        PreparedAdmission::new(
            ctx,
            key,
            raw,
            &Default::default(),
            &catalog::load_yaml(YAML).unwrap(),
        )
        .unwrap()
    }
    async fn counts(&self) -> (i64, i64) {
        let jobs: i64 =
            sqlx::query_scalar("SELECT count(*) FROM labyrinth.jobs WHERE tenant_id=$1")
                .bind(self.context.tenant_id)
                .fetch_one(&self.admin)
                .await
                .unwrap();
        let messages: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pgmq.q_labyrinth_generation_v1 WHERE message->>'tenant_id'=$1",
        )
        .bind(self.context.tenant_id.to_string())
        .fetch_one(&self.admin)
        .await
        .unwrap();
        (jobs, messages)
    }
}
#[tokio::test]
async fn admission_replay_returns_original_job_without_consuming_quota_or_requeueing() {
    let f = Fixture::new(1).await;
    let p = f.prepare("same");
    let a = admit(&f.api, &p).await.unwrap();
    let b = admit(&f.api, &p).await.unwrap();
    assert_eq!(a.job_id, b.job_id);
    assert!(!a.replayed);
    assert!(b.replayed);
    assert_eq!(f.counts().await, (1, 1));
    let payload: serde_json::Value = sqlx::query_scalar(
        "SELECT message FROM pgmq.q_labyrinth_generation_v1 WHERE message->>'tenant_id'=$1",
    )
    .bind(f.context.tenant_id.to_string())
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(payload.as_object().unwrap().len(), 3);
    assert_eq!(payload["job_id"], a.job_id.to_string());
    assert_eq!(payload["schema_version"], 1);
    let mut v: serde_json::Value = serde_json::from_str(RAW).unwrap();
    v["name"] = serde_json::json!("different");
    assert!(matches!(
        admit(
            &f.api,
            &f.prepare_raw(f.context.clone(), "same", &v.to_string())
        )
        .await,
        Err(AdmissionError::IdempotencyConflict)
    ));
    assert!(matches!(
        admit(&f.api, &f.prepare("new")).await,
        Err(AdmissionError::QuotaExceeded)
    ));
}
#[tokio::test]
async fn concurrent_duplicates_have_one_job_and_one_message() {
    let f = Fixture::new(10).await;
    let request = Arc::new(f.prepare("concurrent"));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let p = Arc::clone(&request);
        let pool = f.api.clone();
        handles.push(tokio::spawn(async move { admit(&pool, &p).await.unwrap() }));
    }
    let mut results = Vec::new();
    for h in handles {
        results.push(h.await.unwrap());
    }
    assert_eq!(results.iter().filter(|r| !r.replayed).count(), 1);
    assert!(results.iter().all(|r| r.job_id == results[0].job_id));
    assert_eq!(f.counts().await, (1, 1));
}
#[tokio::test]
async fn concurrent_distinct_projects_share_atomic_tenant_quota() {
    let f = Fixture::new(1).await;
    let first = f.prepare("first");
    let mut second = f.context.clone();
    second.project_id = Uuid::new_v4();
    sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,'second')")
        .bind(second.tenant_id)
        .bind(second.project_id)
        .execute(&f.admin)
        .await
        .unwrap();
    let second = f.prepare_raw(second, "second", RAW);
    let (a, b) = tokio::join!(admit(&f.api, &first), admit(&f.api, &second));
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(
        matches!(a, Err(AdmissionError::QuotaExceeded))
            || matches!(b, Err(AdmissionError::QuotaExceeded))
    );
    assert_eq!(f.counts().await, (1, 1));
}
#[tokio::test]
async fn membership_suspension_and_cross_tenant_projects_are_checked_even_on_replay() {
    let f = Fixture::new(10).await;
    let g = Fixture::new(10).await;
    let mut ctx = f.context.clone();
    ctx.project_id = g.context.project_id;
    assert!(matches!(
        admit(&f.api, &f.prepare_raw(ctx, "foreign", RAW)).await,
        Err(AdmissionError::NotFound)
    ));
    let mut ctx = f.context.clone();
    ctx.actor_id = g.context.actor_id;
    assert!(matches!(
        admit(&f.api, &f.prepare_raw(ctx, "outsider", RAW)).await,
        Err(AdmissionError::Forbidden)
    ));
    let p = f.prepare("replay");
    admit(&f.api, &p).await.unwrap();
    sqlx::query("UPDATE labyrinth.users SET suspended=true WHERE id=$1")
        .bind(f.context.actor_id)
        .execute(&f.admin)
        .await
        .unwrap();
    assert!(matches!(
        admit(&f.api, &p).await,
        Err(AdmissionError::Forbidden)
    ));
    sqlx::query("UPDATE labyrinth.users SET suspended=false WHERE id=$1")
        .bind(f.context.actor_id)
        .execute(&f.admin)
        .await
        .unwrap();
    sqlx::query("UPDATE labyrinth.memberships SET role='viewer' WHERE tenant_id=$1")
        .bind(f.context.tenant_id)
        .execute(&f.admin)
        .await
        .unwrap();
    assert!(matches!(
        admit(&f.api, &p).await,
        Err(AdmissionError::Forbidden)
    ));
    sqlx::query("UPDATE labyrinth.memberships SET role='owner' WHERE tenant_id=$1")
        .bind(f.context.tenant_id)
        .execute(&f.admin)
        .await
        .unwrap();
    sqlx::query("UPDATE labyrinth.tenants SET suspended=true WHERE id=$1")
        .bind(f.context.tenant_id)
        .execute(&f.admin)
        .await
        .unwrap();
    assert!(matches!(
        admit(&f.api, &p).await,
        Err(AdmissionError::Forbidden)
    ));
    assert_eq!(f.counts().await, (1, 1));
}
#[tokio::test]
async fn queue_write_failure_rolls_back_job_and_key_for_a_clean_retry() {
    let f = Fixture::new(1).await;
    let name = format!("reject_{}", f.context.tenant_id.simple());
    let sql = format!(
        "ALTER TABLE pgmq.q_labyrinth_generation_v1 ADD CONSTRAINT {name} CHECK (message->>'tenant_id' <> '{}')",
        f.context.tenant_id
    );
    sqlx::query(&sql).execute(&f.admin).await.unwrap();
    let p = f.prepare("retry");
    let failure = admit(&f.api, &p).await;
    sqlx::query(&format!(
        "ALTER TABLE pgmq.q_labyrinth_generation_v1 DROP CONSTRAINT {name}"
    ))
    .execute(&f.admin)
    .await
    .unwrap();
    assert!(matches!(failure, Err(AdmissionError::Database(_))));
    assert_eq!(f.counts().await, (0, 0));
    assert!(!admit(&f.api, &p).await.unwrap().replayed);
    assert_eq!(f.counts().await, (1, 1));
}
#[tokio::test]
async fn runtime_role_cannot_read_mutate_tables_or_send_queue_messages_directly() {
    let f = Fixture::new(10).await;
    for sql in [
        "SELECT * FROM labyrinth.jobs",
        "UPDATE labyrinth.tenants SET suspended=false",
        "SELECT pgmq.send('labyrinth_generation_v1','{}')",
        "SELECT * FROM labyrinth.admin_audit_logs",
    ] {
        let e = sqlx::query(sql).execute(&f.api).await.unwrap_err();
        assert_eq!(
            e.as_database_error().unwrap().code().as_deref(),
            Some("42501")
        );
    }
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='labyrinth' AND c.relkind='r' AND NOT c.relrowsecurity").fetch_one(&f.admin).await.unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn stale_head_and_immutable_payloads_cannot_be_overridden() {
    let f = Fixture::new(10).await;
    let mut ctx = f.context.clone();
    ctx.expected_head = Some(Uuid::new_v4());
    assert!(matches!(
        admit(&f.api, &f.prepare_raw(ctx, "stale", RAW)).await,
        Err(AdmissionError::StaleHead)
    ));
    let job = admit(&f.api, &f.prepare("valid")).await.unwrap();
    let error = sqlx::query("UPDATE labyrinth.jobs SET requirements='{}' WHERE tenant_id=$1")
        .bind(f.context.tenant_id)
        .execute(&f.admin)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("55000")
    );
    let error = sqlx::query("UPDATE labyrinth.jobs SET state='succeeded' WHERE tenant_id=$1")
        .bind(f.context.tenant_id)
        .execute(&f.admin)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("55000")
    );
    let state: String =
        sqlx::query_scalar("SELECT state FROM labyrinth.jobs WHERE tenant_id=$1 AND id=$2")
            .bind(f.context.tenant_id)
            .bind(job.job_id)
            .fetch_one(&f.admin)
            .await
            .unwrap();
    assert_eq!(state, "queued");
}

#[tokio::test]
async fn incremental_admission_anchors_authoritative_base_digest_and_checks_head_and_catalog() {
    use labyrinth_engine::{
        artifacts::{self, BaseRevision},
        intent,
    };
    let f = Fixture::new(10).await;
    let intent::Readiness::Ready(i) = intent::parse_json(RAW).unwrap() else {
        panic!("fixture unresolved")
    };
    let c = catalog::load_yaml(YAML).unwrap();
    let package = artifacts::generate(&i, &c, &Default::default()).unwrap();
    let revision = Uuid::new_v4();
    let base = BaseRevision::from_verified(engine_revision_id(revision), &package, &c).unwrap();
    let initial = f.prepare("initial");
    let job = admit(&f.api, &initial).await.unwrap();
    sqlx::query("UPDATE labyrinth.jobs SET state='running' WHERE tenant_id=$1")
        .bind(f.context.tenant_id)
        .execute(&f.admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO labyrinth.revisions(tenant_id,project_id,id,job_id,manifest_sha256,catalog_sha256) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(f.context.tenant_id).bind(f.context.project_id).bind(revision).bind(job.job_id).bind(package.manifest_sha256()).bind(&c.version().sha256).execute(&f.admin).await.unwrap();
    sqlx::query("UPDATE labyrinth.jobs SET state='succeeded' WHERE tenant_id=$1")
        .bind(f.context.tenant_id)
        .execute(&f.admin)
        .await
        .unwrap();
    sqlx::query("UPDATE labyrinth.projects SET head_revision_id=$1 WHERE tenant_id=$2 AND id=$3")
        .bind(revision)
        .bind(f.context.tenant_id)
        .bind(f.context.project_id)
        .execute(&f.admin)
        .await
        .unwrap();
    // An identical retry remains replayable after the project head has advanced.
    assert_eq!(admit(&f.api, &initial).await.unwrap().job_id, job.job_id);
    assert!(matches!(
        admit(&f.api, &f.prepare("stale-new")).await,
        Err(AdmissionError::StaleHead)
    ));
    let mut v: serde_json::Value = serde_json::from_str(RAW).unwrap();
    v["update"] = serde_json::to_value(base.update_mode()).unwrap();
    let mut ctx = f.context.clone();
    ctx.expected_head = Some(revision);
    let result = admit(
        &f.api,
        &f.prepare_raw(ctx.clone(), "increment", &v.to_string()),
    )
    .await
    .unwrap();
    let digest: String = sqlx::query_scalar(
        "SELECT base_manifest_sha256::text FROM labyrinth.jobs WHERE tenant_id=$1 AND id=$2",
    )
    .bind(ctx.tenant_id)
    .bind(result.job_id)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(digest, package.manifest_sha256());
    // A different admitted catalog is not a valid implicit migration of the base.
    let other_yaml = YAML.replace("version: 1.2.0", "version: 1.2.1");
    let other = catalog::load_yaml(&other_yaml).unwrap();
    assert_ne!(other.version(), c.version());
    sqlx::query("INSERT INTO labyrinth.catalogs(sha256,semantic_version,yaml,admitted) VALUES($1,$2,$3,true) ON CONFLICT DO NOTHING").bind(&other.version().sha256).bind(&other.version().semantic_version).bind(&other_yaml).execute(&f.admin).await.unwrap();
    let request = PreparedAdmission::new(
        ctx,
        "wrong-catalog",
        &v.to_string(),
        &Default::default(),
        &other,
    )
    .unwrap();
    assert!(matches!(
        admit(&f.api, &request).await,
        Err(AdmissionError::InvalidBase)
    ));
    let error = sqlx::query("UPDATE labyrinth.revisions SET manifest_sha256=$1 WHERE tenant_id=$2")
        .bind("f".repeat(64))
        .bind(f.context.tenant_id)
        .execute(&f.admin)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("55000")
    );
}

#[tokio::test]
async fn composite_foreign_keys_prevent_cross_project_heads_and_artifacts() {
    let f = Fixture::new(10).await;
    let g = Fixture::new(10).await;
    let fake_revision = Uuid::new_v4();
    let error = sqlx::query(
        "UPDATE labyrinth.projects SET head_revision_id=$1 WHERE tenant_id=$2 AND id=$3",
    )
    .bind(fake_revision)
    .bind(f.context.tenant_id)
    .bind(f.context.project_id)
    .execute(&f.admin)
    .await
    .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23503")
    );
    let job = admit(&f.api, &f.prepare("job")).await.unwrap();
    let error=sqlx::query("INSERT INTO labyrinth.revisions(tenant_id,project_id,id,job_id,manifest_sha256,catalog_sha256) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(g.context.tenant_id).bind(g.context.project_id).bind(fake_revision).bind(job.job_id).bind("a".repeat(64)).bind(&catalog::load_yaml(YAML).unwrap().version().sha256).execute(&f.admin).await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23503")
    );
    let error=sqlx::query("INSERT INTO labyrinth.artifacts(tenant_id,project_id,revision_id,name,object_key,sha256,size_bytes) VALUES($1,$2,$3,'topology.json',$4,$5,10)")
        .bind(g.context.tenant_id).bind(g.context.project_id).bind(fake_revision).bind(format!("attempts/{fake_revision}/topology.json")).bind("a".repeat(64)).execute(&f.admin).await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23503")
    );
}

#[tokio::test]
async fn unavailable_catalog_rejects_without_job_or_message() {
    let f = Fixture::new(10).await;
    let version = format!("version: 2.0.{}", Uuid::new_v4().as_u128() as u64);
    let yaml = YAML.replace("version: 1.2.0", &version);
    let c = catalog::load_yaml(&yaml).unwrap();
    sqlx::query("INSERT INTO labyrinth.catalogs(sha256,semantic_version,yaml,admitted) VALUES($1,$2,$3,false)").bind(&c.version().sha256).bind(&c.version().semantic_version).bind(&yaml).execute(&f.admin).await.unwrap();
    let p = PreparedAdmission::new(
        f.context.clone(),
        "unavailable",
        RAW,
        &Default::default(),
        &c,
    )
    .unwrap();
    assert!(matches!(
        admit(&f.api, &p).await,
        Err(AdmissionError::CatalogUnavailable)
    ));
    assert_eq!(f.counts().await, (0, 0));
}
