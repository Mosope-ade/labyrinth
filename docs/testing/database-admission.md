# Database migrations and admission acceptance

## Local setup

Tests use real PostgreSQL and upstream pgmq, not an in-memory queue or SQL mock.
Use a disposable database and trusted migration-owner connection. Do not point this
suite at an existing application/production database: migrations create roles/schemas
and tests deliberately add/remove a queue-table failure constraint.

CI provisions PostgreSQL 17, installs pgmq 1.8.1 from pinned upstream commit
`12462bd29c2e4f59e58ac4840c2dcf0872d8c5c9`, verifies both downloaded file hashes,
and runs the feature-gated database suite. See `.github/workflows/backend.yml`.
That CI workflow is configured; local acceptance does not claim a remote CI run.

For an existing isolated PostgreSQL with pgmq available, create an empty test database,
set `LABYRINTH_TEST_DATABASE_URL` privately, and run:

```sh
cargo test -p labyrinth-api --features postgres-tests --locked
```

A native PostgreSQL 18 installation can use its `extension_control_path` to install
pgmq locally without changing system files. Example setup from the repository root:

```sh
mkdir -p .local/admission-pg/extensions/extension
bash scripts/fetch-pgmq-test-extension.sh .local/admission-pg/extensions/extension
initdb -D .local/admission-pg/data --auth-local=trust --auth-host=scram-sha-256
pg_ctl -D .local/admission-pg/data -l .local/admission-pg/server.log \
  -o "-h '' -k /tmp -p 55439 -c extension_control_path=$PWD/.local/admission-pg/extensions" start
createdb -h /tmp -p 55439 labyrinth_admission_test
```

The trust setting is only for this user's disposable local Unix-socket test cluster;
TCP is disabled. Restrict its data directory and stop it after testing. Use a different
port if occupied. Set the test URL to this cluster's database/user (SQLx accepts
`postgresql:///labyrinth_admission_test?host=/tmp&port=55439`), then run the suite.
Stop with `pg_ctl -D .local/admission-pg/data stop`. PostgreSQL 17 users can use the
CI container setup or install the same verified files in their test server's extension
directory. Hosted Supabase supplies pgmq through its extension integration.

## Covered behavior

- Fresh migrations, checksum history and repeated migrator startup.
- Tenant/project composite foreign keys and valid job state transitions.
- Restricted runtime role; no direct table/queue/audit access; RLS on private tables.
- Current membership, viewer rejection, user/tenant suspension and cross-tenant denial.
- Same-key replay, changed-payload conflict, duplicate concurrent submissions.
- Tenant-wide quota races across different projects.
- Queue failure rolls back both job and idempotency key; retry admits one job/message.
- Stale head rejection; replay remains valid after a head advance.
- Incremental base hash resolved from authorized metadata; catalog mismatch rejects.
- Immutable job input/revision metadata; no silent historical drift.
- Bounded preparation, explicit clarification, option validation, unsupported blueprint
  rejection, order-independent identity and digit-prefixed UUID/core-ID mapping.

The incremental test generates a real verified core package to obtain its base digest
and pins; it seeds publication metadata with the migration owner. It does not claim
that a worker publication API exists. The queue failure test uses a tenant-specific
CHECK constraint on the real queue table, then removes it before testing retry.

## Remaining hosted acceptance

JWT verification and authenticated HTTP admission/status are covered in
[HTTP acceptance](authenticated-http.md). Provisioning APIs, leases/fencing/recovery, hard
process limits, staged storage/publication, cancellation, cleanup/retention and admin
operations remain future slices. Admission is not authorization for direct database
access from an end-user client. Pool/timeouts, numeric production quotas and storage
retention require explicit deployment configuration and measurement.

## Executed acceptance — 2026-11-15

Local PostgreSQL 18.6 and real pgmq 1.8.1; Rust 1.95.0, locked dependencies:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

With the isolated test database URL configured, all **138 tests passed**, zero failed
or ignored: the existing 124 core checks plus 4 preparation and 10 database integration
checks. Database tests apply fresh migrations and rerun the checksum-tracked migrator.
The explicit `migrate` example was also exercised twice against a separate empty
local database. The pinned extension fetch/checksum script and documentation links
were checked. No Supabase project, Render service or production data was changed.

This evidence establishes the initial persistence/admission contract only. It does
not establish hosted network connectivity, authentication flows or worker reliability.
