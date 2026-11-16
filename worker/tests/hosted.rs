#![cfg(feature = "postgres-tests")]
use labyrinth_api::{MIGRATIONS, jobs::*};
use labyrinth_engine::{artifacts, catalog, intent};
use labyrinth_worker::{
    Error, Limits, Result,
    package::{Files, hash},
    runtime::{self, Config},
    storage::{ObjectStore, Storage},
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;
const RAW: &str = include_str!("../../tests/fixtures/intents/enterprise.json");
const YAML: &str = include_str!("../../tests/fixtures/catalog/blueprints.yaml");
struct MemoryStore {
    files: Mutex<Files>,
    fail: AtomicBool,
    corrupt: AtomicBool,
}
impl ObjectStore for MemoryStore {
    async fn put(&self, key: &str, b: &[u8]) -> Result<()> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(Error::Storage);
        }
        let mut files = self.files.lock().unwrap();
        if files.contains_key(key) {
            return Err(Error::Storage);
        }
        files.insert(key.into(), b.to_vec());
        Ok(())
    }
    async fn get(&self, key: &str, limit: usize) -> Result<Vec<u8>> {
        let mut bytes = self
            .files
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or(Error::Storage)?;
        if self.corrupt.load(Ordering::SeqCst) {
            bytes.push(b' ')
        }
        if bytes.len() > limit {
            Err(Error::Storage)
        } else {
            Ok(bytes)
        }
    }
}
async fn state(db: &PgPool, job: Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM labyrinth.jobs WHERE id=$1")
        .bind(job)
        .fetch_one(db)
        .await
        .unwrap()
}
#[tokio::test]
async fn hosted_generation_upload_failure_recovery_publication_and_incremental_update() {
    let root = std::env::var("LABYRINTH_TEST_DATABASE_URL").unwrap();
    let admin = PgPoolOptions::new().connect(&root).await.unwrap();
    let name = format!("hosted_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&root).unwrap();
    url.set_path(&name);
    let db = PgPoolOptions::new().connect(url.as_str()).await.unwrap();
    MIGRATIONS.run(&db).await.unwrap();
    let actor = Uuid::new_v4();
    let tenant = Uuid::new_v4();
    let project = Uuid::new_v4();
    sqlx::query("INSERT INTO labyrinth.users(id) VALUES($1)")
        .bind(actor)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO labyrinth.tenants(id,max_active_jobs) VALUES($1,10)")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO labyrinth.memberships VALUES($1,$2,'owner')")
        .bind(tenant)
        .bind(actor)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,'hosted')")
        .bind(tenant)
        .bind(project)
        .execute(&db)
        .await
        .unwrap();
    let catalog = catalog::load_yaml(YAML).unwrap();
    sqlx::query("INSERT INTO labyrinth.catalogs(sha256,semantic_version,yaml,admitted) VALUES($1,$2,$3,true)").bind(&catalog.version().sha256).bind(&catalog.version().semantic_version).bind(YAML).execute(&db).await.unwrap();
    let worker = PgPoolOptions::new()
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE labyrinth_worker").execute(c).await?;
                Ok(())
            })
        })
        .connect(url.as_str())
        .await
        .unwrap();
    let cfg = Config {
        limits: Limits {
            lease_secs: 60,
            heartbeat_secs: 15,
            deadline_secs: 120,
            cpu_secs: 60,
            memory_bytes: 1024 * 1024 * 1024,
            input_bytes: 2 * 1024 * 1024,
            output_bytes: 32 * 1024 * 1024,
            attempts: 3,
            retry_first_secs: 10,
            retry_second_secs: 60,
        },
        worker_id: "hosted-test".into(),
        prlimit: "/usr/bin/prlimit".into(),
        engine_executable: env!("CARGO_BIN_EXE_labyrinth-worker").into(),
        database_url: String::new(),
        storage: Storage::new(
            "https://unused.example",
            "private".into(),
            "test-only".into(),
        )
        .unwrap(),
    };
    let store = MemoryStore {
        files: Mutex::new(Files::new()),
        fail: AtomicBool::new(true),
        corrupt: AtomicBool::new(false),
    };
    let context = AdmissionContext {
        actor_id: actor,
        tenant_id: tenant,
        project_id: project,
        expected_head: None,
    };
    let job = admit(
        &db,
        &PreparedAdmission::new(
            context.clone(),
            "hosted",
            RAW,
            &Default::default(),
            &catalog,
        )
        .unwrap(),
    )
    .await
    .unwrap()
    .job_id;
    assert!(
        runtime::once_with_store(&worker, &cfg, &store)
            .await
            .unwrap()
    );
    assert_eq!(state(&db, job).await, "retry_wait");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM labyrinth.revisions")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("UPDATE labyrinth.attempts SET retry_at=clock_timestamp()-interval '1 second'")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE pgmq.q_labyrinth_generation_v1 SET vt=clock_timestamp()-interval '1 second'",
    )
    .execute(&db)
    .await
    .unwrap();
    store.fail.store(false, Ordering::SeqCst);
    assert!(
        runtime::once_with_store(&worker, &cfg, &store)
            .await
            .unwrap()
    );
    assert_eq!(state(&db, job).await, "succeeded");
    assert!(
        !runtime::once_with_store(&worker, &cfg, &store)
            .await
            .unwrap()
    );
    let revision: Uuid = sqlx::query_scalar("SELECT head_revision_id FROM labyrinth.projects")
        .fetch_one(&db)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM labyrinth.artifacts")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 13);
    let manifest_key: String =
        sqlx::query_scalar("SELECT object_key FROM labyrinth.artifacts WHERE name='manifest.json'")
            .fetch_one(&db)
            .await
            .unwrap();
    let digest: String = sqlx::query_scalar("SELECT manifest_sha256 FROM labyrinth.revisions")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(hash(&store.files.lock().unwrap()[&manifest_key]), digest);
    // Same trusted package supplies real pins for a hosted incremental update.
    let prefix = manifest_key.strip_suffix("manifest.json").unwrap();
    let files: Files = store
        .files
        .lock()
        .unwrap()
        .iter()
        .filter_map(|(k, v)| k.strip_prefix(prefix).map(|n| (n.to_string(), v.clone())))
        .collect();
    let base = artifacts::BaseRevision::load_saved(
        serde_json::from_value(serde_json::json!(format!("rev-{revision}"))).unwrap(),
        &digest,
        &files,
        &catalog,
    )
    .unwrap();
    let intent::Readiness::Ready(intent) = intent::parse_json(RAW).unwrap() else {
        panic!()
    };
    let mut update = serde_json::to_value(intent.data()).unwrap();
    update["update"] = serde_json::to_value(base.update_mode()).unwrap();
    let raw = serde_json::to_string(&update).unwrap();
    let context = AdmissionContext {
        expected_head: Some(revision),
        ..context
    };
    let incremental = admit(
        &db,
        &PreparedAdmission::new(
            context.clone(),
            "incremental",
            &raw,
            &Default::default(),
            &catalog,
        )
        .unwrap(),
    )
    .await
    .unwrap()
    .job_id;
    runtime::once_with_store(&worker, &cfg, &store)
        .await
        .unwrap();
    assert_eq!(state(&db, incremental).await, "succeeded");
    // Corrupt read-back must never become visible, despite successful uploads.
    let head: Uuid = sqlx::query_scalar("SELECT head_revision_id FROM labyrinth.projects")
        .fetch_one(&db)
        .await
        .unwrap();
    let bad = admit(
        &db,
        &PreparedAdmission::new(
            AdmissionContext {
                expected_head: Some(head),
                ..context
            },
            "corruption",
            RAW,
            &Default::default(),
            &catalog,
        )
        .unwrap(),
    )
    .await
    .unwrap()
    .job_id;
    store.corrupt.store(true, Ordering::SeqCst);
    runtime::once_with_store(&worker, &cfg, &store)
        .await
        .unwrap();
    assert_eq!(state(&db, bad).await, "retry_wait");
    let actual: Uuid = sqlx::query_scalar("SELECT head_revision_id FROM labyrinth.projects")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(actual, head);
    worker.close().await;
    db.close().await;
    sqlx::query(&format!("DROP DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
}
