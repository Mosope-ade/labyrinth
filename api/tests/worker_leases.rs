#![cfg(feature = "postgres-tests")]
use labyrinth_api::MIGRATIONS;
use sqlx::postgres::PgPoolOptions;
#[tokio::test]
async fn worker_role_and_functions_are_restricted() {
    let pool = PgPoolOptions::new()
        .connect(&std::env::var("LABYRINTH_TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    MIGRATIONS.run(&pool).await.unwrap();
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT FROM pg_roles WHERE rolname='labyrinth_worker')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(exists, "worker role missing");
    let allowed: bool = sqlx::query_scalar("SELECT has_function_privilege('labyrinth_api','labyrinth.claim_job(text,integer,integer,integer,integer)','EXECUTE')").fetch_one(&pool).await.unwrap();
    assert!(!allowed);
}

#[tokio::test]
async fn expired_fences_retry_and_atomic_publication() {
    use labyrinth_api::jobs::*;
    use labyrinth_engine::catalog;
    use serde_json::{Value, json};
    use uuid::Uuid;
    let url = std::env::var("LABYRINTH_TEST_DATABASE_URL").unwrap();
    let owner = PgPoolOptions::new().connect(&url).await.unwrap();
    let name = format!("worker_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&owner)
        .await
        .unwrap();
    let mut isolated = reqwest::Url::parse(&url).unwrap();
    isolated.set_path(&name);
    let url = isolated.to_string();
    let db = PgPoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .unwrap();
    MIGRATIONS.run(&db).await.unwrap();
    let tenant = Uuid::new_v4();
    let project = Uuid::new_v4();
    let actor = Uuid::new_v4();
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
    sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,'worker')")
        .bind(tenant)
        .bind(project)
        .execute(&db)
        .await
        .unwrap();
    let yaml = include_str!("../../tests/fixtures/catalog/blueprints.yaml");
    let catalog = catalog::load_yaml(yaml).unwrap();
    sqlx::query("INSERT INTO labyrinth.catalogs(sha256,semantic_version,yaml,admitted) VALUES($1,$2,$3,true) ON CONFLICT DO NOTHING").bind(&catalog.version().sha256).bind(&catalog.version().semantic_version).bind(yaml).execute(&db).await.unwrap();
    let prepared = PreparedAdmission::new(
        AdmissionContext {
            actor_id: actor,
            tenant_id: tenant,
            project_id: project,
            expected_head: None,
        },
        "worker-test",
        include_str!("../../tests/fixtures/intents/enterprise.json"),
        &Default::default(),
        &catalog,
    )
    .unwrap();
    let admitted = admit(&db, &prepared).await.unwrap();
    // Each fixture uses its own database and real pgmq queue.
    let worker = PgPoolOptions::new()
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE labyrinth_worker").execute(c).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    async fn claim(p: &sqlx::PgPool) -> Value {
        sqlx::query_scalar::<_, Option<Value>>(
            "SELECT labyrinth.claim_job('worker-test',60,3,10,60)",
        )
        .fetch_one(p)
        .await
        .unwrap()
        .unwrap_or(Value::Null)
    }
    let first = claim(&worker).await;
    let fence: Uuid = serde_json::from_value(first["fence"].clone()).unwrap();
    let heartbeat = |token| {
        sqlx::query_scalar::<_, bool>("SELECT labyrinth.heartbeat_attempt($1,$2,$3,60)")
            .bind(tenant)
            .bind(admitted.job_id)
            .bind(token)
    };
    assert!(heartbeat(fence).fetch_one(&worker).await.unwrap());
    assert!(claim(&worker).await.is_null());
    sqlx::query("UPDATE labyrinth.attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE fence=$1").bind(fence).execute(&db).await.unwrap();
    sqlx::query("UPDATE pgmq.q_labyrinth_generation_v1 SET vt=clock_timestamp()-interval '1 second' WHERE message->>'job_id'=$1").bind(admitted.job_id.to_string()).execute(&db).await.unwrap();
    assert!(!heartbeat(fence).fetch_one(&worker).await.unwrap());
    assert!(claim(&worker).await.is_null()); // Recovery durably schedules backoff.
    sqlx::query("UPDATE labyrinth.attempts SET retry_at=clock_timestamp()-interval '1 second' WHERE fence=$1").bind(fence).execute(&db).await.unwrap();
    sqlx::query("UPDATE pgmq.q_labyrinth_generation_v1 SET vt=clock_timestamp()-interval '1 second' WHERE message->>'job_id'=$1").bind(admitted.job_id.to_string()).execute(&db).await.unwrap();
    let second = claim(&worker).await;
    let current: Uuid = serde_json::from_value(second["fence"].clone()).unwrap();
    assert_ne!(current, fence);
    assert!(!heartbeat(fence).fetch_one(&worker).await.unwrap());
    let ctx: Value = sqlx::query_scalar("SELECT labyrinth.attempt_context($1,$2,$3)")
        .bind(tenant)
        .bind(admitted.job_id)
        .bind(current)
        .fetch_one(&worker)
        .await
        .unwrap();
    let prefix = ctx["staged_prefix"].as_str().unwrap();
    let names = [
        "bom.json",
        "catalog.json",
        "decisions.json",
        "firewall.json",
        "ipam.json",
        "manifest.json",
        "options.json",
        "provenance.json",
        "requirements.json",
        "routing.json",
        "selection.json",
        "topology.json",
        "verification.json",
    ];
    let hash = "a".repeat(64);
    let files: Value = names
        .iter()
        .map(|n| json!({"name":n,"object_key":format!("{prefix}{n}"),"sha256":hash,"size_bytes":1}))
        .collect();
    let revision = Uuid::new_v4();
    let publish = |token, files: Value| {
        sqlx::query_scalar::<_, bool>("SELECT labyrinth.publish_attempt($1,$2,$3,$4,$5,$6)")
            .bind(tenant)
            .bind(admitted.job_id)
            .bind(token)
            .bind(revision)
            .bind(&hash)
            .bind(files)
    };
    assert!(
        !publish(fence, files.clone())
            .fetch_one(&worker)
            .await
            .unwrap()
    );
    assert!(
        publish(current, json!([]))
            .fetch_one(&worker)
            .await
            .is_err()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM labyrinth.revisions WHERE tenant_id=$1")
            .bind(tenant)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(count, 0);
    assert!(
        publish(current, files.clone())
            .fetch_one(&worker)
            .await
            .unwrap()
    );
    assert!(!publish(current, files).fetch_one(&worker).await.unwrap());
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM labyrinth.artifacts WHERE tenant_id=$1")
            .bind(tenant)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(count, 13);
    let denied = sqlx::query("SELECT * FROM labyrinth.jobs")
        .execute(&worker)
        .await;
    assert!(denied.is_err());

    let prepare_next = |key: &str| {
        PreparedAdmission::new(
            AdmissionContext {
                actor_id: actor,
                tenant_id: tenant,
                project_id: project,
                expected_head: Some(revision),
            },
            key,
            include_str!("../../tests/fixtures/intents/enterprise.json"),
            &Default::default(),
            &catalog,
        )
        .unwrap()
    };
    let retry = admit(&db, &prepare_next("retry-exhaustion")).await.unwrap();
    for number in 1..=3 {
        let handle = claim(&worker).await;
        let token: Uuid = serde_json::from_value(handle["fence"].clone()).unwrap();
        let ok: bool = sqlx::query_scalar("SELECT labyrinth.finish_attempt($1,$2,$3,true,3,10,60)")
            .bind(tenant)
            .bind(retry.job_id)
            .bind(token)
            .fetch_one(&worker)
            .await
            .unwrap();
        assert!(ok);
        let state: String =
            sqlx::query_scalar("SELECT state FROM labyrinth.jobs WHERE tenant_id=$1 AND id=$2")
                .bind(tenant)
                .bind(retry.job_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(state, if number == 3 { "failed" } else { "retry_wait" });
        if number < 3 {
            assert!(claim(&worker).await.is_null());
            sqlx::query("UPDATE labyrinth.attempts SET retry_at=clock_timestamp()-interval '1 second' WHERE fence=$1").bind(token).execute(&db).await.unwrap();
            sqlx::query("UPDATE pgmq.q_labyrinth_generation_v1 SET vt=clock_timestamp()-interval '1 second' WHERE message->>'job_id'=$1").bind(retry.job_id.to_string()).execute(&db).await.unwrap();
        }
    }
    let cancelled = admit(&db, &prepare_next("cancelled")).await.unwrap();
    let handle = claim(&worker).await;
    let token: Uuid = serde_json::from_value(handle["fence"].clone()).unwrap();
    sqlx::query("UPDATE labyrinth.jobs SET state='cancel_requested' WHERE tenant_id=$1 AND id=$2")
        .bind(tenant)
        .bind(cancelled.job_id)
        .execute(&db)
        .await
        .unwrap();
    let renew: bool = sqlx::query_scalar("SELECT labyrinth.heartbeat_attempt($1,$2,$3,60)")
        .bind(tenant)
        .bind(cancelled.job_id)
        .bind(token)
        .fetch_one(&worker)
        .await
        .unwrap();
    assert!(!renew);
    let done: bool = sqlx::query_scalar("SELECT labyrinth.finish_attempt($1,$2,$3,false,3,10,60)")
        .bind(tenant)
        .bind(cancelled.job_id)
        .bind(token)
        .fetch_one(&worker)
        .await
        .unwrap();
    assert!(done);
    let state: String =
        sqlx::query_scalar("SELECT state FROM labyrinth.jobs WHERE tenant_id=$1 AND id=$2")
            .bind(tenant)
            .bind(cancelled.job_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(state, "cancelled");
    // Two accepted redesigns pin the same head; only one may advance it.
    let first_job = admit(&db, &prepare_next("compete-a")).await.unwrap();
    let second_job = admit(&db, &prepare_next("compete-b")).await.unwrap();
    let a = claim(&worker).await;
    let b = claim(&worker).await;
    for (i, (job, handle)) in [(first_job.job_id, a), (second_job.job_id, b)]
        .into_iter()
        .enumerate()
    {
        let token: Uuid = serde_json::from_value(handle["fence"].clone()).unwrap();
        let ctx: Value = sqlx::query_scalar("SELECT labyrinth.attempt_context($1,$2,$3)")
            .bind(tenant)
            .bind(job)
            .bind(token)
            .fetch_one(&worker)
            .await
            .unwrap();
        let prefix = ctx["staged_prefix"].as_str().unwrap();
        let files:Value=names.iter().map(|n|json!({"name":n,"object_key":format!("{prefix}{n}"),"sha256":hash,"size_bytes":1})).collect();
        let ok: bool = sqlx::query_scalar("SELECT labyrinth.publish_attempt($1,$2,$3,$4,$5,$6)")
            .bind(tenant)
            .bind(job)
            .bind(token)
            .bind(Uuid::new_v4())
            .bind(&hash)
            .bind(files)
            .fetch_one(&worker)
            .await
            .unwrap();
        assert_eq!(ok, i == 0);
    }
    worker.close().await;
    db.close().await;
    sqlx::query(&format!("DROP DATABASE {name}"))
        .execute(&owner)
        .await
        .unwrap();
}
