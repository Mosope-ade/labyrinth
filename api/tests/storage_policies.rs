#![cfg(feature = "postgres-tests")]
use labyrinth_api::MIGRATIONS;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
async fn rows(db: &PgPool, who: Uuid, sql: &str) -> Result<u64, sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query("SET LOCAL ROLE authenticated")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT set_config('request.jwt.claim.sub',$1,true)")
        .bind(who.to_string())
        .execute(&mut *tx)
        .await?;
    let result = sqlx::query(sql)
        .execute(&mut *tx)
        .await
        .map(|v| v.rows_affected());
    tx.rollback().await?;
    result
}
async fn count(db: &PgPool, who: Uuid, sql: &str) -> i64 {
    let mut tx = db.begin().await.unwrap();
    sqlx::query("SET LOCAL ROLE authenticated")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('request.jwt.claim.sub',$1,true)")
        .bind(who.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let result = sqlx::query_scalar(sql).fetch_one(&mut *tx).await.unwrap();
    tx.rollback().await.unwrap();
    result
}
#[tokio::test]
async fn technical_storage_policies_enforce_scoped_read_write_delete_and_revocation() {
    let root = std::env::var("LABYRINTH_TEST_DATABASE_URL").expect("isolated PostgreSQL required");
    let admin = PgPoolOptions::new().connect(&root).await.unwrap();
    let database_name = format!("storage_policies_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {database_name}"))
        .execute(&admin)
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&root).unwrap();
    url.set_path(&database_name);
    let db = PgPoolOptions::new().connect(url.as_str()).await.unwrap();
    MIGRATIONS.run(&db).await.unwrap();
    sqlx::raw_sql("DO $$ BEGIN IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname='authenticated') THEN CREATE ROLE authenticated NOLOGIN NOSUPERUSER NOBYPASSRLS; END IF; IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname='anon') THEN CREATE ROLE anon NOLOGIN NOSUPERUSER NOBYPASSRLS; END IF; END $$; CREATE SCHEMA auth; CREATE FUNCTION auth.uid() RETURNS uuid LANGUAGE sql STABLE AS $$ SELECT nullif(current_setting('request.jwt.claim.sub',true),'')::uuid $$; CREATE SCHEMA storage; CREATE TABLE storage.buckets(id text PRIMARY KEY,public boolean); CREATE TABLE storage.objects(bucket_id text,name text,content text DEFAULT 'immutable',PRIMARY KEY(bucket_id,name)); GRANT USAGE ON SCHEMA storage TO authenticated; GRANT SELECT ON storage.buckets TO authenticated; GRANT SELECT,INSERT,UPDATE,DELETE ON storage.objects TO authenticated;").execute(&db).await.unwrap();
    // Provider-owned Storage tables already have RLS enabled. Reproduce that
    // prerequisite in the local provider double, independently of policy bootstrap.
    sqlx::raw_sql("ALTER TABLE storage.objects ENABLE ROW LEVEL SECURITY; ALTER TABLE storage.buckets ENABLE ROW LEVEL SECURITY;")
        .execute(&db).await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO storage.buckets VALUES('engineering-packages',false),('unrelated',false)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!("../../deploy/supabase-storage-policies.sql"))
        .execute(&db)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../../deploy/supabase-storage-policies.sql"))
        .execute(&db)
        .await
        .unwrap();
    let reader = Uuid::new_v4();
    let worker = Uuid::new_v4();
    let human = Uuid::new_v4();
    let tenant = Uuid::new_v4();
    let project = Uuid::new_v4();
    let job = Uuid::new_v4();
    let attempt = Uuid::new_v4();
    let fence = Uuid::new_v4();
    let prefix = format!("attempts/{tenant}/{job}/{attempt}/");
    sqlx::query(
        "INSERT INTO labyrinth.storage_principals(user_id,kind) VALUES($1,'reader'),($2,'worker')",
    )
    .bind(reader)
    .bind(worker)
    .execute(&db)
    .await
    .unwrap();

    assert_eq!(
        count(&db, reader, "SELECT count(*) FROM storage.buckets").await,
        1
    );
    assert_eq!(
        count(&db, human, "SELECT count(*) FROM storage.buckets").await,
        0
    );
    sqlx::query("INSERT INTO labyrinth.users(id) VALUES($1)")
        .bind(human)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO labyrinth.tenants(id,max_active_jobs) VALUES($1,2)")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,'policy fixture')")
        .bind(tenant)
        .bind(project)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO labyrinth.catalogs(sha256,semantic_version,yaml,admitted) VALUES($1,'policy-fixture','fixture',true)").bind("a".repeat(64)).execute(&db).await.unwrap();
    sqlx::query("INSERT INTO labyrinth.jobs(tenant_id,project_id,id,actor_id,idempotency_key,request_sha256,requirements,provenance,options,catalog_sha256,state) VALUES($1,$2,$3,$4,'policy',$5,'{}','{}','{}',$5,'running')").bind(tenant).bind(project).bind(job).bind(human).bind("a".repeat(64)).execute(&db).await.unwrap();
    sqlx::query("INSERT INTO labyrinth.attempts(tenant_id,project_id,job_id,id,attempt_number,fence,worker_id,lease_expires_at,heartbeat_at,staged_prefix) VALUES($1,$2,$3,$4,1,$5,'policy-test',now()+interval '60 seconds',now(),$6)").bind(tenant).bind(project).bind(job).bind(attempt).bind(fence).bind(&prefix).execute(&db).await.unwrap();
    let insert = format!(
        "INSERT INTO storage.objects(bucket_id,name) VALUES('engineering-packages','{prefix}ipam.json')"
    );
    assert_eq!(rows(&db, worker, &insert).await.unwrap(), 1);
    assert!(rows(&db, reader, &insert).await.is_err());
    assert!(rows(&db, human, &insert).await.is_err());
    assert!(rows(&db,worker,"INSERT INTO storage.objects(bucket_id,name) VALUES('engineering-packages','attempts/unknown/bom.json')").await.is_err());
    assert!(
        rows(
            &db,
            worker,
            &insert.replace("engineering-packages", "unrelated")
        )
        .await
        .is_err()
    );
    sqlx::query("INSERT INTO storage.objects(bucket_id,name) VALUES('engineering-packages',$1)")
        .bind(format!("{prefix}bom.json"))
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        count(&db, worker, "SELECT count(*) FROM storage.objects").await,
        1
    );
    assert_eq!(
        count(&db, reader, "SELECT count(*) FROM storage.objects").await,
        0
    );
    assert_eq!(
        rows(&db, worker, "DELETE FROM storage.objects")
            .await
            .unwrap(),
        0
    );
    sqlx::query("UPDATE labyrinth.jobs SET state='succeeded' WHERE id=$1")
        .bind(job)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE labyrinth.attempts SET exit_classification='succeeded' WHERE fence=$1")
        .bind(fence)
        .execute(&db)
        .await
        .unwrap();
    let revision = Uuid::new_v4();
    sqlx::query("INSERT INTO labyrinth.revisions(tenant_id,project_id,id,job_id,manifest_sha256,catalog_sha256) VALUES($1,$2,$3,$4,$5,$5)").bind(tenant).bind(project).bind(revision).bind(job).bind("a".repeat(64)).execute(&db).await.unwrap();
    sqlx::query("INSERT INTO labyrinth.artifacts VALUES($1,$2,$3,'bom.json',$4,$5,2)")
        .bind(tenant)
        .bind(project)
        .bind(revision)
        .bind(format!("{prefix}bom.json"))
        .bind("a".repeat(64))
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        count(&db, reader, "SELECT count(*) FROM storage.objects").await,
        1
    );
    assert_eq!(
        count(&db, human, "SELECT count(*) FROM storage.objects").await,
        0
    );
    assert_eq!(
        rows(&db, worker, "DELETE FROM storage.objects")
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        rows(&db, reader, "DELETE FROM storage.objects")
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        rows(&db, worker, "UPDATE storage.objects SET content='tampered'")
            .await
            .unwrap(),
        0
    );
    assert!(rows(&db, worker, &insert).await.is_err());
    sqlx::query("UPDATE labyrinth.projects SET deleted_at=now()-interval '8 days',lifecycle_epoch=lifecycle_epoch+1 WHERE id=$1").bind(project).execute(&db).await.unwrap();
    assert_eq!(
        count(&db, reader, "SELECT count(*) FROM storage.objects").await,
        0
    );
    assert_eq!(
        rows(&db, worker, "DELETE FROM storage.objects")
            .await
            .unwrap(),
        1
    );
    let orphan_project = Uuid::new_v4();
    let orphan_job = Uuid::new_v4();
    let orphan_attempt = Uuid::new_v4();
    let orphan_fence = Uuid::new_v4();
    let orphan_prefix = format!("attempts/{tenant}/{orphan_job}/{orphan_attempt}/");
    sqlx::query("INSERT INTO labyrinth.projects(tenant_id,id,name) VALUES($1,$2,'unpublished')")
        .bind(tenant)
        .bind(orphan_project)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO labyrinth.jobs(tenant_id,project_id,id,actor_id,idempotency_key,request_sha256,requirements,provenance,options,catalog_sha256,state) VALUES($1,$2,$3,$4,'orphan',$5,'{}','{}','{}',$5,'failed')").bind(tenant).bind(orphan_project).bind(orphan_job).bind(human).bind("a".repeat(64)).execute(&db).await.unwrap();
    sqlx::query("INSERT INTO labyrinth.attempts(tenant_id,project_id,job_id,id,attempt_number,fence,worker_id,lease_expires_at,heartbeat_at,staged_prefix) VALUES($1,$2,$3,$4,1,$5,'policy-orphan',now(),now(),$6)").bind(tenant).bind(orphan_project).bind(orphan_job).bind(orphan_attempt).bind(orphan_fence).bind(&orphan_prefix).execute(&db).await.unwrap();
    sqlx::query(
        "UPDATE labyrinth.attempts SET exit_classification='deterministic_failure' WHERE fence=$1",
    )
    .bind(orphan_fence)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO storage.objects(bucket_id,name) VALUES('engineering-packages',$1)")
        .bind(format!("{orphan_prefix}bom.json"))
        .execute(&db)
        .await
        .unwrap();
    let remove = format!("DELETE FROM storage.objects WHERE name='{orphan_prefix}bom.json'");
    assert_eq!(rows(&db, worker, &remove).await.unwrap(), 0);
    sqlx::query("UPDATE labyrinth.attempts SET ended_at=now()-interval '25 hours' WHERE fence=$1")
        .bind(orphan_fence)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(rows(&db, worker, &remove).await.unwrap(), 1);
    assert_eq!(
        count(&db, reader, "SELECT count(*) FROM storage.objects").await,
        0
    );
    sqlx::query("UPDATE labyrinth.storage_principals SET enabled=false WHERE user_id=$1")
        .bind(worker)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        count(&db, worker, "SELECT count(*) FROM storage.buckets").await,
        0
    );
    assert_eq!(
        rows(&db, worker, "DELETE FROM storage.objects")
            .await
            .unwrap(),
        0
    );
    assert!(
        rows(&db, reader, "SELECT * FROM labyrinth.storage_principals")
            .await
            .is_err()
    );
    sqlx::raw_sql(
        "CREATE POLICY unreviewed ON storage.objects FOR SELECT TO authenticated USING(true)",
    )
    .execute(&db)
    .await
    .unwrap();
    let audits:i64=sqlx::query_scalar("SELECT count(*) FROM labyrinth.admin_audit_logs WHERE action LIKE 'storage_principal_%' AND operator_principal IS NOT NULL").fetch_one(&db).await.unwrap();
    assert_eq!(audits, 3);
    let mut connection = db.acquire().await.unwrap();
    assert!(
        sqlx::raw_sql(include_str!("../../deploy/supabase-storage-policies.sql"))
            .execute(&mut *connection)
            .await
            .is_err()
    );
    sqlx::query("ROLLBACK")
        .execute(&mut *connection)
        .await
        .unwrap();
    drop(connection);
    db.close().await;
    sqlx::query(&format!("DROP DATABASE {database_name}"))
        .execute(&admin)
        .await
        .unwrap();
}
