use crate::{
    Context, Error, Handle, Limits, Result, heartbeat,
    package::{self, EngineInput, Files},
    process,
    storage::{ObjectStore, Storage},
};
use sqlx::{
    ConnectOptions, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;
pub struct Config {
    pub limits: Limits,
    pub worker_id: String,
    pub prlimit: PathBuf,
    pub engine_executable: PathBuf,
    pub database_url: String,
    pub storage: Storage,
}
impl Config {
    pub fn from_env() -> Result<Self> {
        if std::env::var_os("LABYRINTH_STORAGE_TOKEN").is_some() {
            return Err(Error::Invalid);
        }
        let get = |n: &str| std::env::var(n).map_err(|_| Error::Invalid);
        let num = |n: &str| get(n)?.parse::<u64>().map_err(|_| Error::Invalid);
        let limits = Limits {
            lease_secs: i32::try_from(num("LABYRINTH_WORKER_LEASE_SECS")?)
                .map_err(|_| Error::Invalid)?,
            heartbeat_secs: num("LABYRINTH_WORKER_HEARTBEAT_SECS")?,
            deadline_secs: num("LABYRINTH_WORKER_DEADLINE_SECS")?,
            cpu_secs: num("LABYRINTH_WORKER_CPU_SECS")?,
            memory_bytes: num("LABYRINTH_WORKER_MEMORY_BYTES")?,
            input_bytes: usize::try_from(num("LABYRINTH_WORKER_INPUT_BYTES")?)
                .map_err(|_| Error::Invalid)?,
            output_bytes: usize::try_from(num("LABYRINTH_WORKER_OUTPUT_BYTES")?)
                .map_err(|_| Error::Invalid)?,
            attempts: i32::try_from(num("LABYRINTH_WORKER_ATTEMPTS")?)
                .map_err(|_| Error::Invalid)?,
            retry_first_secs: i32::try_from(num("LABYRINTH_WORKER_RETRY_FIRST_SECS")?)
                .map_err(|_| Error::Invalid)?,
            retry_second_secs: i32::try_from(num("LABYRINTH_WORKER_RETRY_SECOND_SECS")?)
                .map_err(|_| Error::Invalid)?,
        };
        limits.validate()?;
        let worker_id = get("LABYRINTH_WORKER_ID")?;
        if worker_id.is_empty() || worker_id.len() > 200 {
            return Err(Error::Invalid);
        }
        let prlimit = PathBuf::from(get("LABYRINTH_PRLIMIT_PATH")?);
        if !prlimit.is_absolute() || !prlimit.is_file() {
            return Err(Error::Invalid);
        }
        Ok(Self {
            limits,
            worker_id,
            prlimit,
            engine_executable: std::env::current_exe()?,
            database_url: get("LABYRINTH_DATABASE_URL")?,
            storage: Storage::technical(
                &get("LABYRINTH_STORAGE_ORIGIN")?,
                get("LABYRINTH_STORAGE_BUCKET")?,
                get("LABYRINTH_STORAGE_API_KEY")?,
                get("LABYRINTH_STORAGE_EMAIL")?,
                get("LABYRINTH_STORAGE_PASSWORD")?,
            )?,
        })
    }
    pub async fn connect(&self) -> Result<PgPool> {
        let opts = self
            .database_url
            .parse::<PgConnectOptions>()
            .map_err(|_| Error::Invalid)?
            .ssl_mode(PgSslMode::VerifyFull)
            .application_name("labyrinth-worker")
            .disable_statement_logging();
        Ok(PgPoolOptions::new().max_connections(2).acquire_timeout(Duration::from_secs(5)).after_connect(|c,_|Box::pin(async move{
            let privileged:bool=sqlx::query_scalar("SELECT rolsuper OR rolcreatedb OR rolcreaterole OR rolbypassrls FROM pg_catalog.pg_roles WHERE rolname=session_user").fetch_one(&mut *c).await?;
            if privileged{return Err(sqlx::Error::Protocol("worker requires unprivileged login".into()))}
            sqlx::query("SET ROLE labyrinth_worker").execute(&mut *c).await?;
            sqlx::query("SET statement_timeout='5s'").execute(&mut *c).await?;
            sqlx::query("SET lock_timeout='2s'").execute(&mut *c).await?;Ok(())
        })).connect_with(opts).await?)
    }
}
async fn execute(
    db: &PgPool,
    h: &Handle,
    cfg: &Config,
    store: &(impl ObjectStore + Sync),
) -> Result<()> {
    let context: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT labyrinth.attempt_context($1,$2,$3)")
            .bind(h.tenant_id)
            .bind(h.job_id)
            .bind(h.fence)
            .fetch_one(db)
            .await?;
    let ctx: Context =
        serde_json::from_value(context.ok_or(Error::LeaseLost)?).map_err(|_| Error::Invalid)?;
    let expected_prefix = format!("attempts/{}/{}/", h.tenant_id, h.job_id);
    if !ctx.staged_prefix.starts_with(&expected_prefix) {
        return Err(Error::Invalid);
    }
    let mut base = None;
    if ctx.base_revision_id.is_some() {
        let mut files = Files::new();
        let mut total = 0;
        for object in &ctx.base_files {
            let bytes = store
                .get(&object.object_key, cfg.limits.output_bytes - total)
                .await?;
            total += bytes.len();
            if bytes.len() as u64 != object.size_bytes
                || package::hash(&bytes) != object.sha256
                || files.insert(object.name.clone(), bytes).is_some()
            {
                return Err(Error::Invalid);
            }
        }
        base = Some(files);
    }
    let request = EngineInput { context: ctx, base };
    let input = serde_json::to_vec(&request).map_err(|_| Error::Invalid)?;
    let ctx = request.context;
    let out = process::run(&cfg.engine_executable, &cfg.prlimit, input, &cfg.limits).await?;
    let files: Files = serde_json::from_slice(&out).map_err(|_| Error::Invalid)?;
    let objects = package::validate(&files, &ctx, cfg.limits.output_bytes)?;
    for object in &objects {
        // Recheck ownership before each external write, including after process suspension.
        heartbeat(db, h, &cfg.limits).await?;
        store.put(&object.object_key, &files[&object.name]).await?;
        let stored = store
            .get(&object.object_key, object.size_bytes as usize)
            .await?;
        if package::hash(&stored) != object.sha256 || stored.len() as u64 != object.size_bytes {
            return Err(Error::Storage);
        }
    }
    heartbeat(db, h, &cfg.limits).await?;
    let published: bool = sqlx::query_scalar("SELECT labyrinth.publish_attempt($1,$2,$3,$4,$5,$6)")
        .bind(h.tenant_id)
        .bind(h.job_id)
        .bind(h.fence)
        .bind(Uuid::new_v4())
        .bind(package::hash(&files["manifest.json"]))
        .bind(serde_json::to_value(objects).map_err(|_| Error::Invalid)?)
        .fetch_one(db)
        .await?;
    if published {
        Ok(())
    } else {
        Err(Error::LeaseLost)
    }
}
pub async fn once(db: &PgPool, cfg: &Config) -> Result<bool> {
    once_with_store(db, cfg, &cfg.storage).await
}
/// Executes the same publication path with an explicitly supplied storage port.
pub async fn once_with_store(
    db: &PgPool,
    cfg: &Config,
    store: &(impl ObjectStore + Sync),
) -> Result<bool> {
    let l = &cfg.limits;
    let value: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT labyrinth.claim_job($1,$2,$3,$4,$5)")
            .bind(&cfg.worker_id)
            .bind(l.lease_secs)
            .bind(l.attempts)
            .bind(l.retry_first_secs)
            .bind(l.retry_second_secs)
            .fetch_one(db)
            .await?;
    let Some(value) = value else { return Ok(false) };
    let h: Handle = serde_json::from_value(value).map_err(|_| Error::Invalid)?;
    let mut interval = tokio::time::interval(Duration::from_secs(l.heartbeat_secs));
    interval.tick().await;
    let result = {
        let work = execute(db, &h, cfg, store);
        tokio::pin!(work);
        let deadline = tokio::time::sleep(Duration::from_secs(l.deadline_secs));
        tokio::pin!(deadline);
        loop {
            tokio::select! {result=&mut work=>break result,_=&mut deadline=>break Err(Error::ResourceLimit),_=interval.tick()=>{if let Err(e)=heartbeat(db,&h,l).await{break Err(e)}}}
        }
    };
    if let Err(error) = result {
        if !matches!(error, Error::LeaseLost) {
            let evidence = if let Error::Engine(report) = &error {
                Some(serde_json::to_value(report).map_err(|_| Error::Invalid)?)
            } else {
                None
            };
            let code = match &error {
                Error::Engine(report) => {
                    if report.outcome == crate::evidence::Outcome::Inconclusive {
                        "resource_limit"
                    } else {
                        "engine_rejected"
                    }
                }
                Error::Invalid => "invalid_input",
                Error::Deterministic => "engine_rejected",
                Error::ResourceLimit => "resource_limit",
                Error::Storage => "storage_unavailable",
                Error::Database(_) => "database_unavailable",
                Error::Io(_) => "process_io",
                Error::LeaseLost => unreachable!(),
            };
            let _: bool = sqlx::query_scalar(
                "SELECT labyrinth.finish_attempt_evidence($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(h.tenant_id)
            .bind(h.job_id)
            .bind(h.fence)
            .bind(code)
            .bind(l.attempts)
            .bind(l.retry_first_secs)
            .bind(l.retry_second_secs)
            .bind(evidence)
            .fetch_one(db)
            .await?;
        }
        // Classification only: never log inputs, credentials, object keys or SQL details.
        eprintln!(
            "worker_attempt_failed class={}",
            match error {
                Error::LeaseLost => "lease_lost",
                Error::Storage => "storage",
                Error::ResourceLimit => "resource_limit",
                Error::Engine(_) => "engine_report",
                Error::Deterministic | Error::Invalid => "deterministic",
                _ => "infrastructure",
            }
        );
    }
    Ok(true)
}
