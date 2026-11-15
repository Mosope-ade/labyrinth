//! Explicit server configuration; credentials are never formatted or logged.
use crate::{auth::AuthConfig, http::HttpLimits};
use sqlx::{
    ConnectOptions, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};
use std::{net::SocketAddr, time::Duration};
#[derive(Debug, thiserror::Error)]
#[error("invalid or missing server configuration: {0}")]
pub struct ConfigError(&'static str);
pub struct RuntimeConfig {
    pub bind: SocketAddr,
    pub auth: AuthConfig,
    pub http: HttpLimits,
    database_url: String,
    max_connections: u32,
    acquire_timeout: Duration,
    statement_ms: u64,
    lock_ms: u64,
}
impl RuntimeConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::load(|name| std::env::var(name).ok())
    }
    fn load(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let required =
            |name: &'static str| get(name).filter(|v| !v.is_empty()).ok_or(ConfigError(name));
        let number = |name: &'static str, min: u64, max: u64| -> Result<u64, ConfigError> {
            required(name)?
                .parse::<u64>()
                .ok()
                .filter(|n| (min..=max).contains(n))
                .ok_or(ConfigError(name))
        };
        let bind = required("LABYRINTH_BIND")?
            .parse()
            .map_err(|_| ConfigError("LABYRINTH_BIND"))?;
        let auth = AuthConfig::new(&required("LABYRINTH_AUTH_ISSUER")?)
            .map_err(|_| ConfigError("LABYRINTH_AUTH_ISSUER"))?;
        let database_url = required("LABYRINTH_DATABASE_URL")?;
        database_url
            .parse::<PgConnectOptions>()
            .map_err(|_| ConfigError("LABYRINTH_DATABASE_URL"))?;
        let max_connections = u32::try_from(number("LABYRINTH_DB_MAX_CONNECTIONS", 1, 32)?)
            .expect("bounded pool count");
        let acquire_timeout =
            Duration::from_millis(number("LABYRINTH_DB_ACQUIRE_TIMEOUT_MS", 100, 10000)?);
        let statement_ms = number("LABYRINTH_DB_STATEMENT_TIMEOUT_MS", 100, 15000)?;
        let lock_ms = number("LABYRINTH_DB_LOCK_TIMEOUT_MS", 100, statement_ms)?;
        let http = HttpLimits {
            max_in_flight: usize::try_from(number("LABYRINTH_HTTP_MAX_IN_FLIGHT", 1, 256)?)
                .expect("bounded capacity"),
            request_timeout: Duration::from_millis(number(
                "LABYRINTH_HTTP_TIMEOUT_MS",
                1000,
                30000,
            )?),
        };
        Ok(Self {
            bind,
            auth,
            http,
            database_url,
            max_connections,
            acquire_timeout,
            statement_ms,
            lock_ms,
        })
    }
    /// Hosted TCP connections always verify TLS certificates/hostnames. Local
    /// development may use a private Unix socket; there is no plaintext TCP flag.
    pub async fn connect(&self) -> Result<PgPool, sqlx::Error> {
        let options = self
            .database_url
            .parse::<PgConnectOptions>()?
            .ssl_mode(PgSslMode::VerifyFull)
            .application_name("labyrinth-api")
            .options([
                ("statement_timeout", self.statement_ms.to_string()),
                ("lock_timeout", self.lock_ms.to_string()),
            ])
            .disable_statement_logging();
        PgPoolOptions::new()
            .max_connections(self.max_connections)
            .acquire_timeout(self.acquire_timeout)
            .after_connect(|connection, _| Box::pin(configure_connection(connection)))
            .connect_with(options)
            .await
    }
}
async fn configure_connection(connection: &mut sqlx::PgConnection) -> Result<(), sqlx::Error> {
    let privileged:bool=sqlx::query_scalar("SELECT rolsuper OR rolcreatedb OR rolcreaterole OR rolbypassrls FROM pg_catalog.pg_roles WHERE rolname=session_user").fetch_one(&mut *connection).await?;
    if privileged {
        return Err(sqlx::Error::Protocol(
            "runtime login must not have administrative privileges".into(),
        ));
    }
    sqlx::query("SET ROLE labyrinth_api")
        .execute(connection)
        .await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn vars() -> std::collections::BTreeMap<&'static str, String> {
        [
            ("LABYRINTH_BIND", "127.0.0.1:3000"),
            (
                "LABYRINTH_AUTH_ISSUER",
                "https://test-project.supabase.co/auth/v1",
            ),
            (
                "LABYRINTH_DATABASE_URL",
                "postgres://runtime:secret@localhost/db",
            ),
            ("LABYRINTH_DB_MAX_CONNECTIONS", "4"),
            ("LABYRINTH_DB_ACQUIRE_TIMEOUT_MS", "1000"),
            ("LABYRINTH_DB_STATEMENT_TIMEOUT_MS", "5000"),
            ("LABYRINTH_DB_LOCK_TIMEOUT_MS", "1000"),
            ("LABYRINTH_HTTP_MAX_IN_FLIGHT", "16"),
            ("LABYRINTH_HTTP_TIMEOUT_MS", "10000"),
        ]
        .into_iter()
        .map(|(k, v)| (k, v.into()))
        .collect()
    }
    #[test]
    fn runtime_configuration_is_explicit_bounded_and_never_echoes_secret_values() {
        let mut values = vars();
        assert!(RuntimeConfig::load(|k| values.get(k).cloned()).is_ok());
        for name in [
            "LABYRINTH_DB_MAX_CONNECTIONS",
            "LABYRINTH_DB_ACQUIRE_TIMEOUT_MS",
            "LABYRINTH_DB_STATEMENT_TIMEOUT_MS",
            "LABYRINTH_DB_LOCK_TIMEOUT_MS",
            "LABYRINTH_HTTP_MAX_IN_FLIGHT",
            "LABYRINTH_HTTP_TIMEOUT_MS",
        ] {
            let mut changed = vars();
            changed.insert(name, "0".into());
            assert!(RuntimeConfig::load(|k| changed.get(k).cloned()).is_err());
        }
        values.insert(
            "LABYRINTH_DATABASE_URL",
            "not a URL containing secret".into(),
        );
        let err = RuntimeConfig::load(|k| values.get(k).cloned())
            .err()
            .unwrap()
            .to_string();
        assert!(!err.contains("secret"));
    }
}

#[cfg(all(test, feature = "postgres-tests"))]
mod database_tests {
    use super::*;
    use sqlx::Connection;
    #[tokio::test]
    async fn privileged_logins_reject_and_unprivileged_sessions_are_restricted_to_api_role() {
        let url = std::env::var("LABYRINTH_TEST_DATABASE_URL")
            .expect("isolated test database URL required");
        let pool = PgPool::connect(&url).await.unwrap();
        crate::MIGRATIONS.run(&pool).await.unwrap();
        let mut connection = sqlx::PgConnection::connect(&url).await.unwrap();
        assert!(configure_connection(&mut connection).await.is_err());
        let role = format!("runtime_test_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!(
            "CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(&format!("GRANT labyrinth_api TO {role}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(&format!("SET SESSION AUTHORIZATION {role}"))
            .execute(&mut connection)
            .await
            .unwrap();
        configure_connection(&mut connection).await.unwrap();
        let current: String = sqlx::query_scalar("SELECT current_user")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert_eq!(current, "labyrinth_api");
        assert!(
            sqlx::query("SELECT * FROM labyrinth.jobs")
                .execute(&mut connection)
                .await
                .is_err()
        );
        connection.close().await.unwrap();
        sqlx::query(&format!("DROP ROLE {role}"))
            .execute(&pool)
            .await
            .unwrap();
    }
}
