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
