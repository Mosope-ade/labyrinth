#![cfg(feature = "postgres-tests")]
use labyrinth_api::MIGRATIONS;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::borrow::Cow;
use uuid::Uuid;
#[tokio::test]
async fn retention_upgrade_preserves_historical_terminals_and_immutable_guards() {
    let root = std::env::var("LABYRINTH_TEST_DATABASE_URL").expect("isolated database required");
    let admin = PgPoolOptions::new().connect(&root).await.unwrap();
    let database_name = format!("retention_upgrade_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {database_name}"))
        .execute(&admin)
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&root).unwrap();
    url.set_path(&database_name);
    let db = PgPoolOptions::new().connect(url.as_str()).await.unwrap();
    let before = sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            MIGRATIONS
                .iter()
                .filter(|m| m.version < 16)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    before.run(&db).await.unwrap();
    let actor = Uuid::new_v4();
    let tenant = Uuid::new_v4();
    let project = Uuid::new_v4();
    let job = Uuid::new_v4();
    sqlx::query("INSERT INTO labyrinth.users(id) VALUES($1)")
        .bind(actor)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO labyrinth.tenants(id,max_active_jobs) VALUES($1,2)")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,'historical')")
        .bind(tenant)
        .bind(project)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO labyrinth.catalogs(sha256,semantic_version,yaml,admitted) VALUES($1,'upgrade-fixture','fixture',true)").bind("a".repeat(64)).execute(&db).await.unwrap();
    sqlx::query("INSERT INTO labyrinth.jobs(tenant_id,project_id,id,actor_id,idempotency_key,request_sha256,requirements,provenance,options,catalog_sha256,state,created_at) VALUES($1,$2,$3,$4,'historical',$5,$6,'{}','{}',$7,'failed',clock_timestamp()-interval '90 days')").bind(tenant).bind(project).bind(job).bind(actor).bind("b".repeat(64)).bind(json!({"private":"preserve until fresh window expires"})).bind("a".repeat(64)).execute(&db).await.unwrap();
    MIGRATIONS.run(&db).await.unwrap();
    MIGRATIONS.run(&db).await.unwrap();
    let retained:bool=sqlx::query_scalar("SELECT ended_at>clock_timestamp()-interval '1 minute' AND requirements<>'{}'::jsonb AND payload_erased_at IS NULL FROM labyrinth.jobs WHERE id=$1").bind(job).fetch_one(&db).await.unwrap();
    assert!(retained);
    let pruned: i32 = sqlx::query_scalar("SELECT labyrinth.prune_job_payloads(100)")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(pruned, 0);
    assert!(
        sqlx::query("UPDATE labyrinth.jobs SET requirements='{}' WHERE id=$1")
            .bind(job)
            .execute(&db)
            .await
            .is_err()
    );
    for function in [
        "labyrinth.publish_attempt_before_retention(uuid,uuid,uuid,uuid,text,jsonb)",
        "labyrinth.heartbeat_before_retention(uuid,uuid,uuid,integer)",
        "labyrinth.context_before_retention(uuid,uuid,uuid)",
        "labyrinth.claim_before_retention(text,integer,integer,integer,integer)",
    ] {
        for role in ["labyrinth_api", "labyrinth_worker"] {
            let allowed: bool =
                sqlx::query_scalar("SELECT has_function_privilege($1,$2,'EXECUTE')")
                    .bind(role)
                    .bind(function)
                    .fetch_one(&db)
                    .await
                    .unwrap();
            assert!(!allowed, "{role} {function}");
        }
    }
    let unchanged: Value =
        sqlx::query_scalar("SELECT requirements FROM labyrinth.jobs WHERE id=$1")
            .bind(job)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(unchanged["private"], "preserve until fresh window expires");
    sqlx::query("INSERT INTO labyrinth.memberships VALUES($1,$2,'owner')")
        .bind(tenant)
        .bind(actor)
        .execute(&db)
        .await
        .unwrap();
    let _: Value = sqlx::query_scalar("SELECT labyrinth.project_lifecycle($1,$2,$3,$3,false)")
        .bind(actor)
        .bind(tenant)
        .bind(project)
        .fetch_one(&db)
        .await
        .unwrap();
    let quota = sqlx::query("SELECT labyrinth.create_project($1,$2,'replacement','Replacement',1)")
        .bind(actor)
        .bind(tenant)
        .execute(&db)
        .await;
    assert_eq!(
        quota.unwrap_err().as_database_error().unwrap().message(),
        "admission_quota_exceeded"
    );
    sqlx::query("UPDATE labyrinth.projects SET deleted_at=clock_timestamp()-interval '8 days' WHERE tenant_id=$1 AND id=$2").bind(tenant).bind(project).execute(&db).await.unwrap();
    let pruned: i32 = sqlx::query_scalar("SELECT labyrinth.prune_job_payloads(100)")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(pruned, 1);
    let purged: bool = sqlx::query_scalar("SELECT labyrinth.finish_project_tombstone()")
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(purged);
    let replacement: Value =
        sqlx::query_scalar("SELECT labyrinth.create_project($1,$2,'replacement','Replacement',1)")
            .bind(actor)
            .bind(tenant)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_ne!(replacement["project_id"], json!(project));
    db.close().await;
    sqlx::query(&format!("DROP DATABASE {database_name}"))
        .execute(&admin)
        .await
        .unwrap();
}
