# Staging deployment runbook

Status: prepared for review. Local tests and configuration validation do not mean
that Render/Supabase deployment, restore, or production capacity is accepted.

1. Confirm region, environment, instance budget and the exact reviewed Git commit.
   The current example proposes Frankfurt for Render and Supabase eu-central-1.
   Preserve dev/staging/prod separation. Do not provision or import a paid Blueprint
   until its resources and charges are explicitly approved.
2. Run engine/backend quality gates and the container workflow. Record the actual
   runtime image digest and base identities; do not deploy a failing/unverified image.
3. Create/select the dedicated Supabase project using operator access. Configure
   Auth/JWKS, SMTP, approved callbacks and private Storage. Complete
   [auth onboarding setup](auth-onboarding.md) and verify independent reader/worker
   storage permissions; never expose server credentials in client bundles.
4. Preflight catalog version uniqueness per [admin runbook](admin-operations.md).
   Apply migrations with LABYRINTH_MIGRATION_DATABASE_URL using the explicit migrate
   tool. Hosted connections require TLS certificate/hostname verification. A failed
   migration reports a redacted error; inspect operator-side database evidence safely.
5. Provision distinct unprivileged database logins granted labyrinth_api,
   labyrinth_worker or labyrinth_maintenance as appropriate. No runtime login may
   have SUPERUSER/CREATEDB/CREATEROLE/BYPASSRLS privileges. Use direct or session-pool
   connectivity with SET ROLE preserved; do not use transaction pooling.
6. Configure exact service variables from the reviewed example and api/worker
   templates. Migration/maintenance secrets belong only to one-off operator jobs.
   Supply private storage credentials separately; verify the provider credential's
   actual scopes/permissions before accepting the deployment.
7. Deploy one API process and one worker process with the distinct commands. API
   binds 0.0.0.0:10000 behind Render TLS. The worker has no public generation listener.
   Ensure /usr/bin/prlimit is present and hard child limits are enforced. Verify
   /health/live and /health/ready without exposing database/provider errors.
8. Complete real magic-link/PKCE login; explicitly onboard twice and verify one
   personal workspace. Use operator access to grant the first admin, validate a
   reviewed generic catalog and promote the exact version/hash. Synthetic fixture
   capacities are not procurement or measured hardware claims.
9. Create a project, browse catalog metadata, validate structured intent, submit a
   job, poll status/attempts, inspect the saved revision and download all artifacts.
   Check the manifest digests. Repeat with the same idempotency key, restart the API
   and worker, and verify there is still at most one published revision per job.
10. Test another user's/tenant's IDs, expired/revoked access, suspension, concurrent
    updates, cancellation, storage outage, deadline/resource failure and orphan cleanup.
    Preserve published bytes during every failure/recovery drill.
11. Test database plus object-storage restoration together. Record limits, load,
    recovery evidence and unresolved failures before considering backend release.

Rollback: redeploy the last accepted immutable image only when its schema compatibility
is verified. Do not reverse/delete immutable revisions, catalogs or artifacts to make
an older binary run. Migrations are additive; incompatible rollback requires a reviewed
compatibility plan. Keep successful package bytes and authoritative manifests intact.

No provider mutations or live customer data are exercised by this runbook itself.
