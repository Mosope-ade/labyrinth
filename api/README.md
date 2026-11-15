# Hosted API application boundary

Implemented: repository-owned SQLx migrations and a bounded admission library backed
by PostgreSQL/pgmq. No HTTP listener, JWT verifier, signup/provisioning workflow, worker
claim API or artifact publication/download endpoint is implemented yet.

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
