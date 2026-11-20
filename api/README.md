# Hosted API application boundary

Implemented: SQLx migrations, PostgreSQL/pgmq admission, Supabase asymmetric JWT
verification, explicit onboarding/projects, authenticated jobs/revisions/downloads,
retention and admin operations. The separate worker handles fenced execution and
artifact publication. Hosted provider acceptance remains pending. See the
[HTTP contract](../docs/contracts/http-jobs-v1.md) and current checkpoints in
[plan.md](../plan.md).

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

Authenticated cancellation and published JSON artifact downloads are specified in
[HTTP V1](../docs/contracts/http-jobs-v1.md). Optional private download transport
settings are listed in [.env.example](.env.example); configure all five together.
Authorization comes from current database membership, never from client object keys.
See [cancellation/download acceptance](../docs/testing/cancellation-downloads.md).

Clients can browse authorized projects, published revisions and artifact inventories
through [project discovery V1](../docs/contracts/project-discovery-v1.md), using bounded
UUID cursor pagination. Project creation/provisioning remains separate work.

Project creation and project detail are specified in
[project creation V1](../docs/contracts/project-creation-v1.md). Creation requires
an explicitly configured LABYRINTH_TENANT_MAX_PROJECTS, current owner/editor access
and an idempotency key. No login or project request silently creates a tenant.

[Authentication/onboarding V1](../docs/contracts/auth-onboarding-v1.md) adds GET /v1/me
and explicit POST /v1/onboarding/personal-workspace. The example policy configures two
active jobs and 50 projects for a new personal tenant. Read
[provider setup](../docs/runbooks/auth-onboarding.md) before hosted rollout. Supabase
and the GUI/CLI auth clients own magic-link/PKCE login, refresh and logout; this API
does not proxy or persist human refresh credentials. Server Storage sessions use
separate technical identities; see [setup](../SUPABASE_SETUP.md).

[Attempt diagnostics](../docs/contracts/job-diagnostics-v1.md) expose safe attempt
history and retry scheduling. No raw worker errors or storage/runtime keys are returned.

[AI-free workflow discovery](../docs/contracts/client-workflow-v1.md) lets clients
select an admitted catalog hash, validate/clarify intent and recover project jobs.

[Admin operations](../docs/contracts/admin-v1.md) use a private operator-managed
registry. See [the runbook](../docs/runbooks/admin-operations.md) for migration preflight,
role maintenance and separate audit retention credentials. Global admin is never a
substitute for private tenant membership.


## Optional AI proposal foundation

`ai::ReviewedProposal` validates provider-output fixtures, exposes clarification or
ready-for-review status and engine defaults, and requires exact candidate/context
confirmation, including displayed generation options, before returning a reviewed
input. It has no provider I/O, HTTP route,
persistence or automatic job admission. The existing structured workflow is unchanged.
Read [AI contracts and limits](../docs/contracts/ai-v1.md). Provider/model, live-call
policies and hosted review lifecycle require separate decisions and implementation.

Export the library schemas from the repository root:

```sh
cargo run --locked -p labyrinth-api --example export_ai_schemas -- proposal
cargo run --locked -p labyrinth-api --example export_ai_schemas -- confirmation
```

Checked-in exports live under `schemas/ai-*-v1.schema.json`; fixture and schema tests
run with `cargo test --locked -p labyrinth-api --test ai_proposals --test ai_schemas`.
These schemas do not advertise a live AI HTTP operation.

[AI policy decisions](../docs/decisions/0012-ai-interaction-data-policy.md) select
Gemini and automatic fallback with notifications; numerical/data recommendations
remain proposed. No provider call or conversation storage is activated.
