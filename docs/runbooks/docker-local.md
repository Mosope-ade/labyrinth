# Run Labyrinth with Docker

Run commands from the repository root. You need Docker Engine/Desktop running,
Python 3, Git and an internet connection for the first image build. Allow RAM
for the configured 512 MiB API + 2 GiB worker limits, plus build/daemon headroom. Rust and an AI
key are not needed on your computer for the offline demonstration. This is a backend
project today: opening the API port will not display a GUI.

## 1. Build and generate a network without accounts

Check Docker first:

```sh
docker version
docker info
umask 077
mkdir -p .local/docker/fixture

docker build --target runtime -f deploy/Dockerfile -t labyrinth-backend:local .
docker build --target fixture --output type=local,dest=.local/docker/fixture -f deploy/Dockerfile .
```

The first build compiles Rust dependencies and can take several minutes. The fixture
stage exports synthetic requirements/catalog data separately from the runtime image.
Now generate a package with no network access:

```sh
docker run --rm -i --network none --read-only \
  --memory 2g --cpus 1 --pids-limit 64 --cap-drop ALL \
  --security-opt no-new-privileges \
  --entrypoint /usr/bin/prlimit labyrinth-backend:local \
  --cpu=60:60 --as=1073741824:1073741824 --core=0:0 -- \
  /app/labyrinth-worker --engine-child \
  < .local/docker/fixture/container-fixture.json \
  > .local/docker/package-wire.json

python3 scripts/verify-container-package.py \
  .local/docker/fixture/container-fixture.json .local/docker/package-wire.json
```

Expected result: `Verified offline 13-artifact container fixture package`. Exit code
2 means rejected/invalid generation; 3 means resource exhaustion. Do not unpack an
unsuccessful or unverified result. The wire format represents file bytes as JSON
arrays; it is not the final human-readable package directory.

After verification, unpack into a new directory:

```sh
python3 - <<'PY'
import json
from pathlib import Path
wire = json.loads(Path('.local/docker/package-wire.json').read_bytes())
output = Path('.local/docker/my-first-network')
output.mkdir(exist_ok=False)
for name, data in wire.items():
    if Path(name).name != name or not name.endswith('.json'):
        raise ValueError('Invalid artifact name')
    (output / name).write_bytes(bytes(data))
print(output)
PY
```

Inspect `topology.json` for devices/links, `ipam.json` for addressing/VLANs,
`firewall.json` and `routing.json` for abstract policy, `bom.json` for generic
archetypes, and `verification.json` for evidence from the five verification passes.
`manifest.json` records identities and file hashes. These are vendor-neutral
engineering outputs, not runnable router configurations or a virtual lab. Fixture
hardware capacities are illustrative, not procurement specifications.

For a repeat run, choose a new unpack directory; no existing package is overwritten.
To change the network, copy/edit the fixture rather than the catalog/fixtures in Git.
The child input is an internal immutable snapshot, including provenance, options and
catalog hash; malformed edits reject. Normal users will submit structured intent
through the API/CLI. The offline demo does not exercise Auth, queueing or persistence.

Optional full image smoke check:

```sh
bash scripts/smoke-container.sh labyrinth-backend:local .local/docker/fixture/container-fixture.json
```

## 2. Prepare the full API/worker environment

The same image contains API, worker and operator tools. These run as separate
processes with separate credentials. This guide connects them to your Supabase
project; it does not silently create a local database or Supabase installation.

Before continuing, follow [SUPABASE_SETUP.md](../../SUPABASE_SETUP.md) and prepare
the dedicated Supabase development project:

- Enable/configure pgmq, asymmetric Auth signing keys and magic-link PKCE callbacks.
- Create the private `engineering-packages` Storage bucket.
- Provision separate unprivileged database LOGINs granted only `labyrinth_api` or
  `labyrinth_worker`; neither may own tables or bypass RLS. Use direct/session-pool
  TLS connections compatible with SET ROLE/prepared statements, not transaction pooling.
- Provide separate read-only API Storage and worker scoped read/write/delete technical identities.
  The worker needs policy-bound deletion, including explicitly deleted published
  content. Never expose these credentials in a browser, CLI or public bucket.

Follow [staging setup](staging-deployment.md) and [Auth setup](auth-onboarding.md).
If this environment is not prepared, stop here and use the offline demo above.

Create ignored local secret files:

```sh
mkdir -p .local/docker
cp api/.env.example .local/docker/api.env
cp worker/.env.example .local/docker/worker.env
chmod 600 .local/docker/api.env .local/docker/worker.env
```

Edit them locally. In `api.env`, set `LABYRINTH_BIND=0.0.0.0:3000`, your issuer,
restricted API database URL and artifact-reader settings. Set LABYRINTH_HTTP_MAX_IN_FLIGHT=4 for this 512 MiB development container. Uncomment all five
artifact settings to enable downloads. In `worker.env`, set the distinct worker DB
URL, Storage origin/bucket/publishable key and technical email/password; keep the documented resource/retention limits.
Docker env files use `NAME=value` lines without shell `export` or surrounding quotes.
Percent-encode reserved characters in database URL passwords. Do not paste secrets
into chat, commit these files, or use the migration owner as a runtime login.

## 3. Apply migrations, including 0016

**Yes: migration 0016 must be attended to before running this backend version against
a database.** It installs deletion/recovery, epoch fencing and retention support.
For a new database, apply the entire migration sequence, not 0016 by itself. For an
existing environment, stop its API/worker and take a verified backup before upgrade.
The migration tool records versions/checksums and applies only pending migrations.
It never runs automatically when an API or worker starts.

Create `.local/docker/migration.env` locally, mode 600, containing only:

```dotenv
LABYRINTH_MIGRATION_DATABASE_URL=postgresql://MIGRATION_OWNER:PERCENT_ENCODED_PASSWORD@DATABASE_HOST:5432/postgres
```

Use the real verified-TLS hostname and migration-owner connection. Then:

```sh
chmod 600 .local/docker/migration.env
docker run --rm --read-only --cap-drop ALL --security-opt no-new-privileges \
  --env-file .local/docker/migration.env \
  labyrinth-backend:local /app/labyrinth-migrate
```

Expected result: `Application migrations applied`. Repeating it is safe when the
stored checksums match. If it fails, do not edit applied SQL or reset migration
history; inspect the operator-side error and fix provisioning/permissions/TLS.
The offline engine demo does not need this migration or any database.

## 4. Start API and worker

```sh
docker run -d --name labyrinth-api --init --read-only \
  --memory 512m --pids-limit 64 --cap-drop ALL --security-opt no-new-privileges \
  -p 127.0.0.1:3000:3000 --env-file .local/docker/api.env \
  labyrinth-backend:local /app/labyrinth-api

docker run -d --name labyrinth-worker --init --read-only \
  --memory 2g --cpus 1 --pids-limit 64 --cap-drop ALL --security-opt no-new-privileges \
  --env-file .local/docker/worker.env \
  labyrinth-backend:local /app/labyrinth-worker

curl -i http://127.0.0.1:3000/health/live
curl -i http://127.0.0.1:3000/health/ready
docker logs --tail 50 labyrinth-api
docker logs --tail 50 labyrinth-worker
```

Both health routes should return 204. Readiness 503 means dependencies/configuration
are not ready; liveness alone is insufficient. API/worker need outbound connections
to Supabase; do not use `--network none` for this mode. No host engine/output volume
is needed: job/revision metadata lives in Postgres and artifacts in private Storage.
The worker has no public port. Exposing the API only on host loopback is intentional.

## 5. What to do after startup

1. Complete real Supabase magic-link/PKCE login in an authorized client. The public
   Auth key is not a bearer access token. Browser login in the product CLI is still
   being built; do not use the service-role key as a user token.
2. Call `GET /v1/me`, then explicitly `POST {}` to
   `/v1/onboarding/personal-workspace`; save tenant_id. Signing in creates no workspace.
3. Create a project with `POST /v1/tenants/{tenant}/projects`, JSON `{"name":"First design"}` and
   an Idempotency-Key. Save project_id. See the exact [project contract](../contracts/project-creation-v1.md).
4. An operator must validate/promote a real catalog version/hash through the admin
   APIs first; onboarding does not select or publish one automatically. Browse
   `/v1/tenants/{tenant}/catalogs` and record the chosen hash. No admin rights are
   automatically granted by signup; see [admin operations](admin-operations.md).
5. Validate intent with `/v1/tenants/{tenant}/intent/validate`; resolve clarification
   requirements. Submit a job with intent, optional options, catalog_sha256 and
   **explicit** `expected_head_revision_id: null` for a new project, plus a fresh
   Idempotency-Key. Reuse that key only when retrying the same submission.
6. Poll scoped job status/attempts. On success, fetch revision metadata and download
   the allowlisted artifacts. Verify hashes against manifest.json before using them.
   On failure, inspect diagnostics/evidence; resource exhaustion is inconclusive.
7. Exercise confirmed project deletion/restore with synthetic data before real use.
   The local worker also runs cleanup: if it is stopped, generation and cleanup wait.

Use [HTTP contracts](../contracts/http-jobs-v1.md), [discovery](../contracts/client-workflow-v1.md),
[retention policy](../contracts/retention-v1.md) and [OpenAPI](../../schemas/http-v1.openapi.json).
No AI key is required for structured generation. Hosted provider login/storage and
backup restore acceptance are separate from the offline smoke test.

## Stop, update and troubleshoot

```sh
docker stop -t 30 labyrinth-api labyrinth-worker
docker rm labyrinth-api labyrinth-worker
```

This removes containers, not Supabase data. Rebuild from the desired reviewed commit,
apply pending migrations with services stopped, then recreate containers. Do not
rollback code past a database contract it needs; retain immutable migration history.

- Cannot connect to Docker: start Docker Desktop/Engine and verify your Docker context.
- API exits immediately: fill all required API variables; an unconfigured image is
  designed to fail closed. Binding 127.0.0.1 inside the container prevents port forwarding.
- Port 3000 is occupied: change the host mapping to 127.0.0.1:3001:3000 and use
  port 3001 in curl; leave the container bind on 3000.
- Database failure: verify TLS host, URL escaping, connection mode and role grants.
- Worker exits: check Storage privacy/permissions and every explicit limit setting.
- Jobs remain queued: check worker logs/connectivity, catalog admission and tenant quota.
- 401/403: use a current user access token, then check current membership/suspension.
- 503 downloads: enable the complete five-field artifact-reader variable group with read permission.
- No GUI: expected; this image serves the backend only.

Do not add a bucket-wide expiry rule: active published designs do not expire.
Application backups still require separately configured expiry, an independent
lifecycle ledger and a tested restore gate. No paid services or provider resources
are created by these Docker commands apart from your explicitly prepared environment.

If your provider requires a separate database CA, save its verified **public** CA as
.local/docker/db-ca.crt, make that certificate readable (0644; keep env files 0600),
and add sslrootcert=/app/db-ca.crt to the DB URL query. Add this option to each DB-
using docker run command, including migration/API/worker:

```sh
--mount type=bind,src="$PWD/.local/docker/db-ca.crt",dst=/app/db-ca.crt,readonly
```

This is an extra option on the existing command, not a standalone shell command.
Verify the CA against the provider; do not substitute sslmode=disable.
