# Backend deployment preparation

This directory is preparation, not a completed hosted deployment. No paid resource,
provider role, secret, bucket or migration is created automatically by validation.

## Container

Build from the repository root:

```sh
docker build --target runtime -f deploy/Dockerfile -t labyrinth-backend:review .
docker build --target fixture --output type=local,dest=/tmp/labyrinth-fixture -f deploy/Dockerfile .
bash scripts/smoke-container.sh labyrinth-backend:review /tmp/labyrinth-fixture/container-fixture.json
```

One pinned image contains separate API/worker executables and operator tools. It runs
as UID/GID 65532 without the Rust compiler, source tree, fixture data or build cache.
The default command is /app/labyrinth-api; the worker service overrides it with
/app/labyrinth-worker. Different service credentials preserve the existing role
boundary; sharing image bytes does not grant one service another role's authority.
Migrations and audit retention are explicit operator commands, never startup hooks.

Official Docker Hub tag metadata and image-index digests are recorded in
base-images.json. Rust dependencies use Cargo.lock and --locked. Review base-image
updates deliberately and retain final built image digests. OS package installation
uses the configured Debian repositories at build time: this is not a promise of
byte-identical rebuilds on different dates.

The smoke check requires no network or hosted credential inside containers. It
checks non-root execution, runs the real engine under CPU/address-space/container
limits with a public synthetic fixture, validates the fixed package inventory and
all digests/five passes, and requires API startup to fail without configuration.
CI builds this image and exports smoke input from a separate stage; fixture input
is not copied into the production runtime.

## Staging proposal

render.staging.example.yaml proposes Frankfurt, one 0.5 CPU/512 MiB API and one
1 CPU/2 GiB worker, separate service environments, explicit limits, a private bucket
and automatic deployment disabled. It is a review example, not root render.yaml.
Region/budget approval and provider access are still required before provisioning.
Creating a Blueprint can incur charges even when automatic deployment is off.

The worker's child address-space ceiling remains 1 GiB; 2 GiB service memory leaves
initial parent headroom. API concurrency is initially four, not an unlimited capacity
claim. Production sizing/HA, load/SLO and recovery targets still need measurements.
Supabase/SMTP/provider fees are separate. See [Render pricing](https://render.com/pricing)
and the [Blueprint specification](https://render.com/docs/blueprint-spec).

Validate without creating resources:

```sh
python3 -m venv .local/deploy-validation
.local/deploy-validation/bin/python -m pip install --only-binary=:all: --index-url https://pypi.org/simple --require-hashes -r deploy/validation/requirements.lock
.local/deploy-validation/bin/python scripts/validate-render-blueprint.py deploy/render.staging.example.yaml
```

Validation uses a hash-pinned official provider schema, safe YAML parsing, duplicate
key/environment checks and redacted errors. Tool dependencies are independently
wheel/hash-locked in validation/; none enter the Rust production image. If Render
changes the schema hash, review the provider change before updating its lock.

## Environment and operator sequence

Follow [the staging runbook](../docs/runbooks/staging-deployment.md). Use distinct
migration-owner, API LOGIN, worker LOGIN and maintenance LOGIN credentials. Runtime
logins must be unprivileged and assume only their designated group role. Keep owner
and maintenance secrets out of API/worker environments. Use a dedicated application
Supabase project; migrations intentionally restrict queue exposure.

Use verified TLS for hosted PostgreSQL. Choose a direct/session-pooling connection
that supports session-level SET ROLE and the application's prepared statements;
transaction pooling is incompatible with the existing after-connect role boundary.
Verify endpoint/network compatibility with the selected Render/Supabase regions.

The API readiness route is /health/ready; liveness remains /health/live. Readiness
checks restricted role/schema dependencies and fresh/refreshable JWKS, with bounded
shared capacity and no operational response details. Storage configuration, actual
email/PKCE flows and provider restore/load drills remain hosted acceptance checks.
