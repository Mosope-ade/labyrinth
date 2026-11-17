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
            "public-test-key".into(),
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
    let report:Option<String>=sqlx::query_scalar("SELECT diagnostic_code FROM labyrinth.attempts WHERE tenant_id=$1 AND job_id=$2 ORDER BY attempt_number DESC LIMIT 1").bind(tenant).bind(job).fetch_one(&db).await.unwrap();
    assert_eq!(report.as_deref(), Some("storage_unavailable"));
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

    // The new attempt is young: neither uploaded nor absent objects are touched.
    assert!(
        !labyrinth_worker::cleanup::once(&worker, &store, 86400)
            .await
            .unwrap()
    );
    assert!(
        labyrinth_worker::cleanup::once(&worker, &store, 86399)
            .await
            .is_err()
    );
    let published: Vec<String> =
        sqlx::query_scalar("SELECT object_key FROM labyrinth.artifacts WHERE tenant_id=$1")
            .bind(tenant)
            .fetch_all(&db)
            .await
            .unwrap();
    let preserved: Files = {
        let files = store.files.lock().unwrap();
        published
            .iter()
            .map(|k| (k.clone(), files[k].clone()))
            .collect()
    };
    sqlx::query("UPDATE labyrinth.attempts SET ended_at=clock_timestamp()-interval '25 hours',cleanup_due_at=clock_timestamp() WHERE tenant_id=$1").bind(tenant).execute(&db).await.unwrap();
    // Two claimers cannot concurrently own the same attempt cleanup lease.
    let (a, b) = tokio::join!(
        sqlx::query_scalar::<_, Option<serde_json::Value>>("SELECT labyrinth.claim_cleanup(86400)")
            .fetch_one(&worker),
        sqlx::query_scalar::<_, Option<serde_json::Value>>("SELECT labyrinth.claim_cleanup(86400)")
            .fetch_one(&worker)
    );
    let claims: Vec<_> = [a.unwrap(), b.unwrap()].into_iter().flatten().collect();
    assert_eq!(claims.len(), 2); // Failed upload and failed read-back: two distinct attempts.
    assert_ne!(claims[0]["fence"], claims[1]["fence"]);
    // Expire both claims; a replacement token fences a stale cleanup owner.
    sqlx::query("UPDATE labyrinth.attempts SET cleanup_lease_until=clock_timestamp()-interval '1 second' WHERE tenant_id=$1 AND cleanup_token IS NOT NULL").bind(tenant).execute(&db).await.unwrap();
    for claim in &claims {
        let fence: Uuid = serde_json::from_value(claim["fence"].clone()).unwrap();
        let token: Uuid = serde_json::from_value(claim["token"].clone()).unwrap();
        let prefix: Option<String> = sqlx::query_scalar("SELECT labyrinth.check_cleanup($1,$2)")
            .bind(fence)
            .bind(token)
            .fetch_one(&worker)
            .await
            .unwrap();
        assert!(prefix.is_none());
    }
    // Partial deletion is a retryable operation; no published bytes may change.
    store.fail.store(true, Ordering::SeqCst);
    assert!(
        labyrinth_worker::cleanup::once(&worker, &store, 86400)
            .await
            .is_err()
    );
    store.fail.store(false, Ordering::SeqCst);
    sqlx::query("UPDATE labyrinth.attempts SET cleanup_due_at=clock_timestamp() WHERE tenant_id=$1 AND exit_classification<>'succeeded'").bind(tenant).execute(&db).await.unwrap();
    for _ in 0..2 {
        assert!(
            labyrinth_worker::cleanup::once(&worker, &store, 86400)
                .await
                .unwrap()
        );
    }
    assert!(
        !labyrinth_worker::cleanup::once(&worker, &store, 86400)
            .await
            .unwrap()
    );
    for (key, bytes) in &preserved {
        assert_eq!(store.files.lock().unwrap().get(key), Some(bytes));
    }
    // A late stale upload is removed on a subsequent daily tombstone sweep.
    let prefix: String = sqlx::query_scalar(
        "SELECT staged_prefix FROM labyrinth.attempts WHERE tenant_id=$1 AND job_id=$2",
    )
    .bind(tenant)
    .bind(bad)
    .fetch_one(&db)
    .await
    .unwrap();
    let late_key = format!("{prefix}manifest.json");
    store
        .files
        .lock()
        .unwrap()
        .insert(late_key.clone(), b"late".to_vec());
    sqlx::query("UPDATE labyrinth.attempts SET cleanup_due_at=clock_timestamp() WHERE tenant_id=$1 AND job_id=$2").bind(tenant).bind(bad).execute(&db).await.unwrap();
    assert!(
        labyrinth_worker::cleanup::once(&worker, &store, 86400)
            .await
            .unwrap()
    );
    assert!(!store.files.lock().unwrap().contains_key(&late_key));
    // Normal inserts cannot attach a published revision to a terminal failed prefix.
    let blocked=sqlx::query("INSERT INTO labyrinth.artifacts(tenant_id,project_id,revision_id,name,object_key,sha256,size_bytes) VALUES($1,$2,$3,'manifest.json',$4,$5,1)").bind(tenant).bind(project).bind(head).bind(&late_key).bind("a".repeat(64)).execute(&db).await;
    assert_eq!(
        blocked.unwrap_err().as_database_error().unwrap().message(),
        "terminal_attempt_cannot_publish"
    );
    let allowed:bool=sqlx::query_scalar("SELECT has_function_privilege('labyrinth_api','labyrinth.claim_cleanup(integer)','EXECUTE')").fetch_one(&db).await.unwrap();
    assert!(!allowed);

    // Publication of a fresh attempt and cleanup of an old prefix can run together.
    store.corrupt.store(false, Ordering::SeqCst);
    sqlx::query("UPDATE pgmq.q_labyrinth_generation_v1 SET vt=clock_timestamp()+interval '1 hour' WHERE message->>'job_id'=$1").bind(bad.to_string()).execute(&db).await.unwrap();
    let race_job = admit(
        &db,
        &PreparedAdmission::new(
            AdmissionContext {
                actor_id: actor,
                tenant_id: tenant,
                project_id: project,
                expected_head: Some(head),
            },
            "cleanup-publication-race",
            RAW,
            &Default::default(),
            &catalog,
        )
        .unwrap(),
    )
    .await
    .unwrap()
    .job_id;
    sqlx::query("UPDATE labyrinth.attempts SET cleanup_due_at=clock_timestamp() WHERE tenant_id=$1 AND job_id=$2").bind(tenant).bind(bad).execute(&db).await.unwrap();
    let (generation, cleanup) = tokio::join!(
        runtime::once_with_store(&worker, &cfg, &store),
        labyrinth_worker::cleanup::once(&worker, &store, 86400)
    );
    assert!(generation.unwrap());
    assert!(cleanup.unwrap());
    assert_eq!(state(&db, race_job).await, "succeeded");
    for (key, bytes) in &preserved {
        assert_eq!(store.files.lock().unwrap().get(key), Some(bytes));
    }
    let failed_fence: Uuid =
        sqlx::query_scalar("SELECT fence FROM labyrinth.attempts WHERE tenant_id=$1 AND job_id=$2")
            .bind(tenant)
            .bind(bad)
            .fetch_one(&db)
            .await
            .unwrap();
    let stale_publish: bool =
        sqlx::query_scalar("SELECT labyrinth.publish_attempt($1,$2,$3,$4,$5,'[]'::jsonb)")
            .bind(tenant)
            .bind(bad)
            .bind(failed_fence)
            .bind(Uuid::new_v4())
            .bind("a".repeat(64))
            .fetch_one(&worker)
            .await
            .unwrap();
    assert!(!stale_publish);
    let reset =
        sqlx::query("UPDATE labyrinth.attempts SET exit_classification=NULL WHERE fence=$1")
            .bind(failed_fence)
            .execute(&db)
            .await;
    assert_eq!(
        reset.unwrap_err().as_database_error().unwrap().message(),
        "terminal_attempt_immutable"
    );
    // A real child search-budget failure survives fencing/persistence without publication.
    let evidence_project = Uuid::new_v4();
    sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,'evidence')")
        .bind(tenant)
        .bind(evidence_project)
        .execute(&db)
        .await
        .unwrap();
    let options = artifacts::GenerationOptions {
        selection_search_states: 1,
        ..Default::default()
    };
    let evidence_job = admit(
        &db,
        &PreparedAdmission::new(
            AdmissionContext {
                actor_id: actor,
                tenant_id: tenant,
                project_id: evidence_project,
                expected_head: None,
            },
            "evidence",
            RAW,
            &options,
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
    assert_eq!(state(&db, evidence_job).await, "failed");
    let evidence: serde_json::Value = sqlx::query_scalar(
        "SELECT engine_evidence FROM labyrinth.attempts WHERE tenant_id=$1 AND job_id=$2",
    )
    .bind(tenant)
    .bind(evidence_job)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(evidence["outcome"], "inconclusive");
    assert!(evidence["error"].is_object());
    let revisions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM labyrinth.revisions WHERE tenant_id=$1 AND project_id=$2",
    )
    .bind(tenant)
    .bind(evidence_project)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(revisions, 0);
    // Retention never erases a young failed payload or any active published design.
    let pruned: i32 = sqlx::query_scalar("SELECT labyrinth.prune_job_payloads(100)")
        .fetch_one(&worker)
        .await
        .unwrap();
    assert_eq!(pruned, 0);
    let mut age = db.begin().await.unwrap();
    sqlx::query("SET LOCAL session_replication_role=replica")
        .execute(&mut *age)
        .await
        .unwrap();
    sqlx::query("UPDATE labyrinth.jobs SET ended_at=clock_timestamp()-interval '31 days' WHERE tenant_id=$1 AND id=$2").bind(tenant).bind(evidence_job).execute(&mut *age).await.unwrap();
    age.commit().await.unwrap();
    let pruned: i32 = sqlx::query_scalar("SELECT labyrinth.prune_job_payloads(100)")
        .fetch_one(&worker)
        .await
        .unwrap();
    assert_eq!(pruned, 1);
    let payload: serde_json::Value =
        sqlx::query_scalar("SELECT requirements FROM labyrinth.jobs WHERE tenant_id=$1 AND id=$2")
            .bind(tenant)
            .bind(evidence_job)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(payload, serde_json::json!({}));
    let evidence: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT engine_evidence FROM labyrinth.attempts WHERE tenant_id=$1 AND job_id=$2",
    )
    .bind(tenant)
    .bind(evidence_job)
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(evidence.is_none());
    let payload: serde_json::Value =
        sqlx::query_scalar("SELECT requirements FROM labyrinth.jobs WHERE tenant_id=$1 AND id=$2")
            .bind(tenant)
            .bind(job)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_ne!(payload, serde_json::json!({}));

    let erased_key:bool=sqlx::query_scalar("SELECT idempotency_key IS NULL AND idempotency_key_sha256 IS NOT NULL FROM labyrinth.jobs WHERE tenant_id=$1 AND id=$2").bind(tenant).bind(evidence_job).fetch_one(&db).await.unwrap();
    assert!(erased_key);
    let replay = admit(
        &db,
        &PreparedAdmission::new(
            AdmissionContext {
                actor_id: actor,
                tenant_id: tenant,
                project_id: evidence_project,
                expected_head: None,
            },
            "evidence",
            RAW,
            &options,
            &catalog,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.job_id, evidence_job);
    // A second live project proves explicit purge cannot touch unrelated published bytes.
    store.fail.store(false, Ordering::SeqCst);
    store.corrupt.store(false, Ordering::SeqCst);
    let preserved_project = Uuid::new_v4();
    sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,'preserve')")
        .bind(tenant)
        .bind(preserved_project)
        .execute(&db)
        .await
        .unwrap();
    let preserved_job = admit(
        &db,
        &PreparedAdmission::new(
            AdmissionContext {
                actor_id: actor,
                tenant_id: tenant,
                project_id: preserved_project,
                expected_head: None,
            },
            "retention-preserved",
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
    assert_eq!(state(&db, preserved_job).await, "succeeded");
    let keys: Vec<String> = sqlx::query_scalar(
        "SELECT object_key FROM labyrinth.artifacts WHERE tenant_id=$1 AND project_id=$2",
    )
    .bind(tenant)
    .bind(preserved_project)
    .fetch_all(&db)
    .await
    .unwrap();
    let protected: Files = {
        let files = store.files.lock().unwrap();
        keys.iter().map(|k| (k.clone(), files[k].clone())).collect()
    };
    sqlx::query(
        "INSERT INTO labyrinth.project_creations VALUES($1,$2,'hosted-project',$3,'hosted')",
    )
    .bind(tenant)
    .bind(actor)
    .bind(project)
    .execute(&db)
    .await
    .unwrap();
    let interrupted_project = Uuid::new_v4();
    sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,'interrupt')")
        .bind(tenant)
        .bind(interrupted_project)
        .execute(&db)
        .await
        .unwrap();
    let interrupted = admit(
        &db,
        &PreparedAdmission::new(
            AdmissionContext {
                actor_id: actor,
                tenant_id: tenant,
                project_id: interrupted_project,
                expected_head: None,
            },
            "delete-in-flight",
            RAW,
            &Default::default(),
            &catalog,
        )
        .unwrap(),
    )
    .await
    .unwrap()
    .job_id;
    let interrupt = DeleteAfterFirstWrite {
        store: &store,
        db: &db,
        actor,
        tenant,
        project: interrupted_project,
        writes: std::sync::atomic::AtomicUsize::new(0),
    };
    assert!(
        runtime::once_with_store(&worker, &cfg, &interrupt)
            .await
            .unwrap()
    );
    assert_eq!(interrupt.writes.load(Ordering::SeqCst), 1);
    assert_eq!(state(&db, interrupted).await, "cancelled");
    let before = store.files.lock().unwrap().clone();
    let _: serde_json::Value =
        sqlx::query_scalar("SELECT labyrinth.project_lifecycle($1,$2,$3,$3,false)")
            .bind(actor)
            .bind(tenant)
            .bind(project)
            .fetch_one(&db)
            .await
            .unwrap();
    let _ = labyrinth_worker::retention::once(&worker, &store)
        .await
        .unwrap();
    assert_eq!(*store.files.lock().unwrap(), before);
    let _: serde_json::Value =
        sqlx::query_scalar("SELECT labyrinth.project_lifecycle($1,$2,$3,$3,true)")
            .bind(actor)
            .bind(tenant)
            .bind(project)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(*store.files.lock().unwrap(), before);
    let _: serde_json::Value =
        sqlx::query_scalar("SELECT labyrinth.project_lifecycle($1,$2,$3,$3,false)")
            .bind(actor)
            .bind(tenant)
            .bind(project)
            .fetch_one(&db)
            .await
            .unwrap();
    sqlx::query("UPDATE labyrinth.projects SET deleted_at=clock_timestamp()-interval '8 days' WHERE tenant_id=$1 AND id=$2").bind(tenant).bind(project).execute(&db).await.unwrap();
    assert!(
        sqlx::query("SELECT labyrinth.project_lifecycle($1,$2,$3,$3,true)")
            .bind(actor)
            .bind(tenant)
            .bind(project)
            .execute(&db)
            .await
            .is_err()
    );
    store.fail.store(true, Ordering::SeqCst);
    assert!(
        labyrinth_worker::retention::once(&worker, &store)
            .await
            .is_err()
    );
    let purged: bool = sqlx::query_scalar(
        "SELECT purged_at IS NOT NULL FROM labyrinth.projects WHERE tenant_id=$1 AND id=$2",
    )
    .bind(tenant)
    .bind(project)
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(!purged);
    store.fail.store(false, Ordering::SeqCst);
    sqlx::query("UPDATE labyrinth.attempts SET project_erase_due=clock_timestamp()-interval '1 second' WHERE tenant_id=$1 AND project_id=$2").bind(tenant).bind(project).execute(&db).await.unwrap();
    for _ in 0..16 {
        if !labyrinth_worker::retention::once(&worker, &store)
            .await
            .unwrap()
        {
            break;
        }
    }
    let purged:bool=sqlx::query_scalar("SELECT purged_at IS NOT NULL AND name='[deleted]' FROM labyrinth.projects WHERE tenant_id=$1 AND id=$2").bind(tenant).bind(project).fetch_one(&db).await.unwrap();
    assert!(purged);
    let erased_project_name: String = sqlx::query_scalar(
        "SELECT name FROM labyrinth.project_creations WHERE tenant_id=$1 AND project_id=$2",
    )
    .bind(tenant)
    .bind(project)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(erased_project_name, "[deleted]");
    {
        let files = store.files.lock().unwrap();
        for key in &published {
            assert!(!files.contains_key(key));
        }
        for (key, bytes) in &protected {
            assert_eq!(&files[key], bytes);
        }
    }
    // A late stale upload is removed by a repeat erasure sweep, without reopening access.
    let late = published[0].clone();
    store
        .files
        .lock()
        .unwrap()
        .insert(late.clone(), b"late".to_vec());
    sqlx::query("UPDATE labyrinth.attempts SET project_erase_due=clock_timestamp()-interval '1 second' WHERE tenant_id=$1 AND project_id=$2").bind(tenant).bind(project).execute(&db).await.unwrap();
    for _ in 0..16 {
        if !labyrinth_worker::retention::once(&worker, &store)
            .await
            .unwrap()
        {
            break;
        }
    }
    assert!(!store.files.lock().unwrap().contains_key(&late));
    let erased_key:bool=sqlx::query_scalar("SELECT idempotency_key IS NULL AND idempotency_key_sha256 IS NOT NULL FROM labyrinth.project_creations WHERE tenant_id=$1 AND project_id=$2").bind(tenant).bind(project).fetch_one(&db).await.unwrap();
    assert!(erased_key);
    let replay = sqlx::query("SELECT labyrinth.create_project($1,$2,'hosted-project','hosted',50)")
        .bind(actor)
        .bind(tenant)
        .execute(&db)
        .await;
    assert_eq!(
        replay.unwrap_err().as_database_error().unwrap().message(),
        "admission_not_found"
    );
    // Reconcile the latest independent ledger before a restored database can serve users.
    // A latest ledger may also say that an owner already restored a retained design.
    let _: serde_json::Value =
        sqlx::query_scalar("SELECT labyrinth.project_lifecycle($1,$2,$3,$3,false)")
            .bind(actor)
            .bind(tenant)
            .bind(preserved_project)
            .fetch_one(&db)
            .await
            .unwrap();
    let _: serde_json::Value =
        sqlx::query_scalar("SELECT labyrinth.project_lifecycle($1,$2,$3,$3,true)")
            .bind(actor)
            .bind(tenant)
            .bind(preserved_project)
            .fetch_one(&db)
            .await
            .unwrap();
    let ledger: serde_json::Value =
        sqlx::query_scalar("SELECT labyrinth.export_project_lifecycle(NULL,NULL,100)")
            .fetch_one(&db)
            .await
            .unwrap();
    let entry = ledger
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["project_id"] == serde_json::json!(project))
        .unwrap();
    let epoch: i64 = entry["epoch"].as_str().unwrap().parse().unwrap();
    for role in ["labyrinth_api", "labyrinth_worker", "labyrinth_maintenance"] {
        let allowed:bool=sqlx::query_scalar("SELECT has_function_privilege($1,'labyrinth.export_project_lifecycle(uuid,uuid,integer)','EXECUTE')").bind(role).fetch_one(&db).await.unwrap();
        assert!(!allowed);
        let allowed:bool=sqlx::query_scalar("SELECT has_function_privilege($1,'labyrinth.reconcile_project_lifecycle(uuid,uuid,bigint,timestamptz,boolean)','EXECUTE')").bind(role).fetch_one(&db).await.unwrap();
        assert!(!allowed);
    }
    sqlx::query("UPDATE labyrinth.projects SET deleted_at=NULL,purged_at=NULL,lifecycle_epoch=0 WHERE tenant_id=$1 AND id=$2").bind(tenant).bind(project).execute(&db).await.unwrap();
    let maintenance = PgPoolOptions::new()
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE labyrinth_recovery")
                    .execute(c)
                    .await?;
                Ok(())
            })
        })
        .connect(url.as_str())
        .await
        .unwrap();
    let apply = || {
        sqlx::query_scalar::<_, bool>(
            "SELECT labyrinth.reconcile_project_lifecycle($1,$2,$3,$4::text::timestamptz,true)",
        )
        .bind(tenant)
        .bind(project)
        .bind(epoch)
        .bind(entry["deleted_at"].as_str().unwrap())
    };
    assert!(apply().fetch_one(&maintenance).await.unwrap());
    assert!(!apply().fetch_one(&maintenance).await.unwrap());
    let blocked:bool=sqlx::query_scalar("SELECT deleted_at IS NOT NULL AND lifecycle_epoch>$3 FROM labyrinth.projects WHERE tenant_id=$1 AND id=$2").bind(tenant).bind(project).bind(epoch).fetch_one(&db).await.unwrap();
    assert!(blocked);
    for _ in 0..16 {
        if !labyrinth_worker::retention::once(&worker, &store)
            .await
            .unwrap()
        {
            break;
        }
    }
    for (key, bytes) in &protected {
        assert_eq!(&store.files.lock().unwrap()[key], bytes);
    }
    let active_entry = ledger
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["project_id"] == serde_json::json!(preserved_project))
        .unwrap();
    assert!(active_entry["deleted_at"].is_null());
    let active_epoch: i64 = active_entry["epoch"].as_str().unwrap().parse().unwrap();
    sqlx::query("UPDATE labyrinth.projects SET lifecycle_epoch=$3,deleted_at=clock_timestamp() WHERE tenant_id=$1 AND id=$2").bind(tenant).bind(preserved_project).bind(active_epoch-1).execute(&db).await.unwrap();
    let restored: bool =
        sqlx::query_scalar("SELECT labyrinth.reconcile_project_lifecycle($1,$2,$3,NULL,false)")
            .bind(tenant)
            .bind(preserved_project)
            .bind(active_epoch)
            .fetch_one(&maintenance)
            .await
            .unwrap();
    assert!(restored);
    let active:bool=sqlx::query_scalar("SELECT deleted_at IS NULL AND lifecycle_epoch>$3 FROM labyrinth.projects WHERE tenant_id=$1 AND id=$2").bind(tenant).bind(preserved_project).bind(active_epoch).fetch_one(&db).await.unwrap();
    assert!(active);
    for (key, bytes) in &protected {
        assert_eq!(&store.files.lock().unwrap()[key], bytes);
    }
    maintenance.close().await;
    worker.close().await;
    db.close().await;
    sqlx::query(&format!("DROP DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
}

impl labyrinth_worker::cleanup::OrphanStore for MemoryStore {
    async fn delete(&self, keys: &[String]) -> Result<()> {
        let mut files = self.files.lock().unwrap();
        for (i, key) in keys.iter().enumerate() {
            files.remove(key);
            if i == 5 && self.fail.load(Ordering::SeqCst) {
                return Err(Error::Storage);
            }
        }
        Ok(())
    }
}

struct DeleteAfterFirstWrite<'a> {
    store: &'a MemoryStore,
    db: &'a PgPool,
    actor: Uuid,
    tenant: Uuid,
    project: Uuid,
    writes: std::sync::atomic::AtomicUsize,
}
impl ObjectStore for DeleteAfterFirstWrite<'_> {
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        self.store.put(key, bytes).await?;
        if self.writes.fetch_add(1, Ordering::SeqCst) == 0 {
            let _: serde_json::Value =
                sqlx::query_scalar("SELECT labyrinth.project_lifecycle($1,$2,$3,$3,false)")
                    .bind(self.actor)
                    .bind(self.tenant)
                    .bind(self.project)
                    .fetch_one(self.db)
                    .await?;
        }
        Ok(())
    }
    async fn get(&self, key: &str, limit: usize) -> Result<Vec<u8>> {
        self.store.get(key, limit).await
    }
}
