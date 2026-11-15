# Hosted API application boundary

Implemented: repository-owned SQLx migrations, PostgreSQL/pgmq admission, Supabase
asymmetric JWT verification, authenticated Axum submission/status routes and a runnable
server. Signup/provisioning, worker claim/execution and artifact publication/download
endpoints remain pending. See the [HTTP contract](../docs/contracts/http-jobs-v1.md).

`jobs::PreparedAdmission::new` validates structured intent and options using the core,
normalizes a versioned request identity and preserves defaults/provenance.
`jobs::admit` calls one restricted PostgreSQL function that authorizes the actor,
checks head/catalog/quota, resolves the base digest, inserts the job and enqueues it
atomically. This is readiness/admission, not verified engineering generation.

## Apply migrations

Use a dedicated migration owner on an approved database with permission to install
`pgmq` and create the restricted group role. The owner must not be the API login.
Supply the migration URL through your local secret environment, then run:

```sh
cargo run -p labyrinth-api --example migrate --locked
```

The example requires `LABYRINTH_MIGRATION_DATABASE_URL`. SQLx checks migration history
and checksums under a database migration lock. Startup can safely re-run completed
migrations; never edit a deployed migration. These initial files are not yet deployed.
Use the appropriate direct/session connection for migrations, with TLS on hosted DBs.

Provision a separate backend login with only `labyrinth_api` membership, then use
`SET ROLE labyrinth_api` on each pooled connection (as the integration tests do).
Do not use Supabase's migration/postgres login or a broad service role in the API.
Explicitly set bounded pool size, connection/acquisition timeout, lock timeout and
statement timeout when adding the deployment/server configuration. No database URL
or credential belongs in browser/CLI builds or source control.

User IDs mirror verified Supabase subjects; administrative provisioning must create
users/tenants/memberships/projects/catalogs. Tenant quota is required, not defaulted.
Only owner/editor members submit; viewer members cannot. No admin rights are inferred.

## Tests

Pure request tests run with `cargo test --workspace --locked`.
Database tests require an **isolated disposable database** whose owner can migrate:

```sh
cargo test -p labyrinth-api --features postgres-tests --locked
```

Set `LABYRINTH_TEST_DATABASE_URL` first. Missing configuration is an explicit failure
when the feature is enabled; the database tests are not ignored. Tests insert isolated
UUID-scoped fixtures and deliberately inject queue failures; never use production.
See [database test setup](../docs/testing/database-admission.md).

See [job contract](../docs/contracts/hosted-jobs-v1.md) and
[admission ADR](../docs/decisions/0004-transactional-admission.md) for authorization,
locking, idempotency, revision identities and remaining worker obligations.

## Run the authenticated server

Configure the required variables in [the API template](.env.example) through your
shell or secret manager; the process does not auto-load `.env` files. Then run:

```sh
cargo run -p labyrinth-api --locked
```

Use an asymmetric Supabase project issuer and an unprivileged LOGIN with only
`labyrinth_api` membership. The server rejects administrative database logins, requires
verified TLS for hosted TCP database connections, selects the restricted role on each
connection and applies bounded statement/lock/pool acquisition times. Local development
can use a private Unix socket. Apply migrations separately with their owner before
startup. The server never provisions membership or promotes an authenticated user.

`POST /v1/tenants/{tenant}/projects/{project}/jobs` admits structured JSON;
`GET /v1/tenants/{tenant}/projects/{project}/jobs/{job}` polls durable scoped state.
One bearer access token is required. Process liveness is public at `/health/live`.
No auth bypass, cookie session, wildcard CORS, refresh/login endpoint or synchronous
generator is installed. See [authentication ADR](../docs/decisions/0005-authenticated-job-http.md)
for cache/rotation and session-revocation limits. Supabase/Render connectivity and
production configuration still need deployment acceptance.
