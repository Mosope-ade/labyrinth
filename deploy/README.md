# Backend deployment preparation

This directory is preparation, not a completed hosted deployment. No paid resource,
provider role, secret, bucket or migration is created automatically by validation.

For a step-by-step local run and what to do afterward, see the
[Docker guide](../docs/runbooks/docker-local.md) and [Supabase setup](../SUPABASE_SETUP.md).

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

The current zero-budget target is one Render **Free web service** for the API,
Supabase **Free** for PostgreSQL/Auth/private Storage/queue, and the existing worker
on an operator-controlled development machine. Cloudflare Pages remains the future
frontend target; GitHub Releases remains the CLI distribution target. No paid worker,
SMTP subscription, custom domain or AI API subscription is required for structured
JSON generation. The Frankfurt region remains a proposal, subject to availability.

`render.staging.example.yaml` contains only the free API; it deliberately declares no
Render worker. Render has no free background-worker plan. Run the unchanged worker
locally with its separate credentials and explicit limits from `worker/.env.example`.
Its child address-space limit remains 1 GiB; allow additional parent memory on the
host. This is a development arrangement, not an always-available hosted service.
Jobs wait in the queue while the local worker is offline; accepted leases/retries and
publication fencing still apply. Do not put worker credentials into the API service.

Render Free services sleep after 15 idle minutes and have shared monthly usage
limits. Supabase Free currently includes 500 MB database storage and 1 GB object
storage and may pause after a week of inactivity. Check quotas before demos; use
synthetic data and export independent backups. Never delete published artifacts as
an automatic quota workaround. Auth email delivery/provider limits still require
live acceptance; do not promise unrestricted public signup from the free setup.

Select Free explicitly and keep paid add-ons disabled. Prefer no payment method;
where one already exists, configure provider spend controls and confirm their scope
before deployment. Reaching free quotas may interrupt service. Do not deploy the
worker as a fake web service or add keep-alive traffic to bypass provider limits.
No resources have been provisioned by this configuration change.

Provider references (checked 2026-11-16): [Render Free](https://render.com/docs/free),
[Supabase pricing](https://supabase.com/pricing).

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
