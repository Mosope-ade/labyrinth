# Supabase setup for Labyrinth — free development environment

Start with **Phase A**, then complete Phase B. You can prepare them independently
and tell me when they are done.
Do not send database passwords, secret keys, tokens or connection strings in chat.
No frontend is available yet. The source-built CLI now supports browser PKCE login; see [CLI usage](cli/README.md). The offline Docker engine demo
works now without Supabase: see [Docker guide](docs/runbooks/docker-local.md).

## Phase A: project, database, Auth and private bucket

### 1. Create the free project

In the Supabase dashboard, choose an organization on **Free**, then create one
project for Labyrinth development. Choose Frankfurt if available (the deployment
proposal); use the same region consistently in later setup. Generate a strong
unique database password and save it in your password manager. Do not enable paid
compute, PITR or other add-ons. Wait for the project to become healthy.

Record these non-secret identifiers privately in your working notes:

- Project reference and HTTPS project URL: `https://PROJECT_REF.supabase.co`.
- Auth issuer: the project URL followed by `/auth/v1`.
- Publishable API key (identifies the application; it is not a user access token).
- Direct or **session** pooler hostname, port and connection username from **Connect**.

Do not confuse an API key, an Auth access JWT and a Postgres password. They serve
three different protocols. Supabase Free can pause inactive projects and has quotas;
verify current limits before demos. This development setup is not an uptime guarantee.

### 2. Enable the queue extension, keep it private

Open Integrations / Queues (dashboard labels can change) and enable **pgmq**.
Check from the SQL editor:

```sql
select extname, extversion from pg_extension where extname = 'pgmq';
```

Keep “Expose Queues via PostgREST” disabled. Do not grant queue access to anon or
ordinary authenticated clients. Do not create the Labyrinth generation queue by
hand: application migrations create and restrict it. Keep the `labyrinth` schema
out of the Data API exposed-schema list; the backend uses restricted direct SQL.

### 3. Prepare Auth

Under Authentication, enable email authentication and use **asymmetric JWT signing
keys** compatible with RS256 or ES256, and make the intended asymmetric key active
following the provider rotation procedure. The backend verifies the project's public
JWKS; it does not accept a legacy shared HS256 signing secret.

Check the public keys endpoint (no secrets are required):

```sh
curl --fail https://PROJECT_REF.supabase.co/auth/v1/.well-known/jwks.json
```

The response must contain public signing keys. Never paste a private signing key
into application config. Human sign-in will use email magic links with PKCE.
For the CLI, configure `http://127.0.0.1:54327/callback?state=*` as described in
[CLI usage](cli/README.md); the wildcard covers only correlation query data, with
fixed loopback host/port/path. Do not use wildcard hosted browser redirect origins. Built-in SMTP
has provider restrictions; verify allowed test recipients before expecting emails.
Production SMTP and real login/refresh/logout tests remain later acceptance work.

### 4. Create the private Storage bucket

Open Storage and create `engineering-packages` with **Public bucket disabled**.
Do not add a broad authenticated-user download policy. Normal human users will
receive files through the Labyrinth API after project authorization, not by accessing
Storage directly. Do not configure bucket-wide time-based expiry: retained published
designs must survive until their owner explicitly deletes the project.

Leave Storage credential/policy wiring for Phase B below. A service/secret key
bypasses RLS, so using one does not prove reader-only or bucket-scoped permissions.

### 5. Build the Docker image and apply all migrations

From the repository root:

```sh
mkdir -p .local/docker
docker build --target runtime -f deploy/Dockerfile -t labyrinth-backend:local .
```

Create `.local/docker/migration.env` locally with mode 600 and one entry:

```dotenv
LABYRINTH_MIGRATION_DATABASE_URL=postgresql://postgres:PERCENT_ENCODED_PASSWORD@VERIFIED_DATABASE_HOST:5432/postgres
```

Use the migration-owner connection from Connect. If using a shared session pooler,
its username may include the project reference: copy the dashboard's format and
substitute the correct role. Percent-encode URL password characters. Use **session**
mode/direct connectivity, not transaction pooling. The migration tool enforces full
TLS certificate/hostname verification. If a CA certificate is needed, configure and
mount the provider's verified CA; do not disable TLS verification.

```sh
chmod 600 .local/docker/migration.env
docker run --rm --read-only --cap-drop ALL --security-opt no-new-privileges \
  --env-file .local/docker/migration.env \
  labyrinth-backend:local /app/labyrinth-migrate
```

Expected output: `Application migrations applied`. This applies 0001–0016 to a new
database, or only pending migrations to an existing database. **0016 is required**
for the current backend. It adds retention/recovery and old-worker fencing. Do not
run that SQL file alone or edit already-applied migration checksums. Existing
environments need services stopped and a verified backup before upgrade.

Verify through the SQL editor:

```sql
select version, description, success from public._sqlx_migrations order by version;
select to_regprocedure('labyrinth.project_lifecycle(uuid,uuid,uuid,uuid,boolean)');
```

All versions should show success, including 16, and the function lookup must be
non-null. If the history table is not in public, locate the schema selected by the
migration connection rather than creating another history table.

### 6. Create restricted runtime database logins

Migrations create NOLOGIN capability groups. Create separate LOGINs and grant only
the appropriate group. These are Postgres roles, **not Supabase Auth users**.
Using an operator connection, run:

```sql
create role labyrinth_api_login login noinherit nosuperuser nocreatedb nocreaterole nobypassrls;
create role labyrinth_worker_login login noinherit nosuperuser nocreatedb nocreaterole nobypassrls;
grant labyrinth_api to labyrinth_api_login;
grant labyrinth_worker to labyrinth_worker_login;
```

Set each password with your PostgreSQL client's secure interactive password prompt,
for example `\password labyrinth_api_login` and `\password labyrinth_worker_login`
inside psql (install a PostgreSQL client if needed). These are psql commands, not SQL-editor statements. Use unique generated
passwords; do not put them in shell arguments, Git or a saved SQL script. If you use
another operator interface, verify how it handles secret entry/history first.

Test each login over verified TLS with psql. API session:

```sql
set role labyrinth_api;
select current_user;
select * from labyrinth.projects limit 1;
```

Expected: current_user is labyrinth_api; the direct-table query is **denied**.
The worker login should similarly assume labyrinth_worker and be denied direct
project-table access. Neither login may assume the other's role, migration owner,
labyrinth_maintenance or labyrinth_recovery. Do not grant schema ownership, table
DML, superuser or RLS bypass to make a failing check pass.

Create audit-maintenance/offline-recovery logins only when you need those operator
tools. Their group capabilities are distinct; neither belongs in API/worker secrets.

### 7. Prepare local configuration and optionally test API health

```sh
cp api/.env.example .local/docker/api.env
cp worker/.env.example .local/docker/worker.env
chmod 600 .local/docker/api.env .local/docker/worker.env
```

Edit api.env: bind `0.0.0.0:3000`, set the exact Auth issuer and the **API login's**
DB URL. Keep the five optional artifact settings commented out until Phase B.
Edit worker.env: set the **worker login's** URL and Storage origin/bucket. Leave it
stopped until scoped Storage sessions are ready. Docker env files contain unquoted
`NAME=value` lines, not shell export statements.

You can now run just the API using the Docker guide and check liveness/readiness.
With artifact configuration absent, downloads correctly return 503. This is not a
full generation or user-login acceptance test. Never substitute the owner DB URL
when a restricted login fails.

**Tell me Phase A is ready when:** pgmq is enabled/private, the bucket is private,
migration 16 is applied, both restricted DB logins pass their permission checks,
and the API can become ready. Report failed step names/error categories only.

## Phase B: dedicated Storage identities and automatic sessions

This design is approved and implemented. Human sign-in remains magic-link PKCE.
Technical identities use noninteractive password login to Supabase Auth, then refresh
rotating sessions automatically on use. This is the ordinary Auth REST token API,
not the optional OAuth 2.1 server feature. Tokens stay in backend memory, not files.

### 8. Create two technical Auth accounts

In Authentication → Users → Add user, create two **distinct**, server-only accounts:
one reader and one worker. Use email aliases you control and different randomly
generated passwords (24+ characters recommended). Confirm these operator-created
technical accounts so they can authenticate without an interactive email step.
Do not use your personal account. Do not onboard them into application workspaces
or grant them global admin. Record their two different Auth user UUIDs.

Keep ordinary email confirmation enabled for human accounts. Leave session defaults
unless you have verified compatibility; use access-token TTL up to one day (the
usual hour is appropriate). Verify concurrent technical sessions remain usable
before running multiple replicas. Never put technical passwords in browser/CLI code.

### 9. Install the reviewed private Storage policies

First inspect existing policies:

```sql
select schemaname, tablename, policyname, roles, cmd, qual, with_check
from pg_policies where schemaname='storage' and tablename in ('objects','buckets');
```

From the reviewed repository commit, open `deploy/supabase-storage-policies.sql` and
run its complete contents in the SQL editor as the operator. It requires migration
0016 and the private engineering-packages bucket. It refuses unreviewed existing
Storage policies, rather than silently combining them with a broad allow rule.
After an error, rollback the failed transaction and investigate; do not remove other
policies blindly or disable RLS. The file can be applied again safely after success.

Then register the **real two UUIDs**, editing the placeholders locally:

```sql
begin;
insert into labyrinth.storage_principals(user_id,kind)
values ('READER_AUTH_USER_UUID'::uuid,'reader'),
       ('WORKER_AUTH_USER_UUID'::uuid,'worker');
commit;
```

The registry is private; human users cannot register themselves. Both identities
can read metadata for the one private bucket. The reader can read only published
files in active projects. The worker can write/read only live-attempt inventory
paths, read retained published files, and delete only policy-eligible orphan/deleted
project inventories. Neither can overwrite objects or access another bucket. Human
JWTs get no direct Storage access. Keep labyrinth out of exposed Data API schemas;
the authenticated schema-usage grant only supports the narrowly scoped RLS helper.

### 10. Fill the component secret files

Use the modern `sb_publishable_...` project key. Runtime technical identity config
rejects secret/service-role keys and legacy anon JWT gateway keys. In api.env,
uncomment and set all **five** fields:

```dotenv
LABYRINTH_ARTIFACT_STORAGE_ORIGIN=https://PROJECT_REF.supabase.co
LABYRINTH_ARTIFACT_STORAGE_BUCKET=engineering-packages
LABYRINTH_ARTIFACT_STORAGE_API_KEY=sb_publishable_REPLACE_LOCALLY
LABYRINTH_ARTIFACT_STORAGE_EMAIL=YOUR_READER_TECHNICAL_EMAIL
LABYRINTH_ARTIFACT_STORAGE_PASSWORD=REPLACE_LOCALLY
```

In worker.env set the equivalent `LABYRINTH_STORAGE_ORIGIN`, `_BUCKET`, `_API_KEY`,
`_EMAIL`, `_PASSWORD` values using the **worker account**, while preserving its
separate worker DB URL. These Auth passwords are literal env values, not URI-encoded
DB passwords. Keep files mode 600, use unquoted Docker env-file values, and never
commit or print them. Remove old `...STORAGE_TOKEN` entries entirely: static runtime
token config is deliberately rejected instead of expiring unexpectedly later.

The gateway key and bearer JWT are sent separately. Login/refresh requests are
coalesced per process, bounded, redirect-disabled and redacted. A rejected refresh
can reauthenticate once using the protected technical password. Provider outages
back off; expired tokens are not accepted as valid cache entries. Clock/suspend
expiry and Storage 401 responses trigger a new session. Storage mutations are never
silently replayed. After restart, the process logs in fresh rather than requiring a
saved rotating refresh token.

### 11. Run and verify

Follow [Docker startup](docs/runbooks/docker-local.md), with API concurrency set to
4 for the 512 MiB development container. Worker startup must verify a private bucket;
missing grants or wrong technical credentials fail closed. Health alone does not
prove Storage authorization. Complete synthetic generation/download and negative
scope tests before real data. Local tests exercise session transport and SQL RLS
logic; actual Supabase JWT/gateway/Storage behavior still requires this live check.

For an immediate Storage-identity revocation:

```sql
update labyrinth.storage_principals set enabled=false
where user_id='COMPROMISED_TECHNICAL_UUID'::uuid;
```

This blocks subsequent RLS requests even with an unexpired JWT. Rotate to a **new
technical UUID** and credentials, register it, update backend secrets and restart.
Keep the old UUID disabled: merely changing its password and re-enabling the same
UUID can make old unexpired JWTs usable again. Do not grant yourself a service key
or broad policy to work around a failed scope check.

**Tell me when A/B are prepared** (without secrets). Then we will finish real human
PKCE login, explicit onboarding, catalog promotion, structured generation and
manifest-verified downloads, plus revocation/retention/backup restore acceptance.
The CLI login client is available from source; actual provider acceptance remains
pending your setup. Structured engineering commands are available from source; see CLI usage. No AI key is required.

These are Labyrinth app projects, not Supabase infrastructure projects. Never use
“Delete Supabase project” to test Labyrinth's owner-confirmed deletion endpoint.

## Official references (checked 2026-11-17)

- [Database connections](https://supabase.com/docs/guides/database/connecting-to-postgres)
- [Queues quickstart](https://supabase.com/docs/guides/queues/quickstart)
- [API keys](https://supabase.com/docs/guides/getting-started/api-keys)
- [Storage access control](https://supabase.com/docs/guides/storage/security/access-control)
- [Passwordless email](https://supabase.com/docs/guides/auth/auth-email-passwordless)
- [PKCE](https://supabase.com/docs/guides/auth/sessions/pkce-flow)
- [Backups](https://supabase.com/docs/guides/platform/backups)

- [Supabase Auth REST schema](https://github.com/supabase/auth/blob/master/openapi.yaml)
