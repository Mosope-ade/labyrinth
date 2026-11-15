# Authenticated HTTP acceptance

Scope: Supabase user JWT verification, Axum submission/status, restricted Postgres
read functions and server configuration. No hosted credentials or AI key are needed.

## Evidence map

- Real ES256 and RS256 signatures verify; forged signatures, HMAC, wrong issuer/audience,
  expired/future/missing claims, non-user roles, anonymous users and missing sessions
  reject. JWT metadata cannot grant tenant/admin permission.
- A local JWKS HTTP issuer exercises the actual fetch/decoder/cache. Duplicate/empty/
  wrong-use/wrong-key-type sets fail; concurrent misses coalesce to one refresh;
  rotation replaces expired keys; fresh cached
  keys survive provider failure, expired keys fail closed, misses respect cooldown.
  Plaintext test transport is cfg(test)-only; the binary accepts HTTPS issuer config.
- A live Axum listener receives a real HTTP POST with a signed token and persists one
  job/message in real PostgreSQL/pgmq. Retry and a rebuilt router return the same job.
- HTTP/database tests cover viewer read-only behavior, outsider/forged-admin metadata,
  membership revocation, suspension, cross-project/missing jobs, stale heads, quota,
  clarification, invalid intent, duplicate JSON fields and published revision identity.
- Inconsistent success metadata never returns a result; closed storage/auth-key outages
  return sanitized 503 errors and do not fabricate job progress or package contents.
- Payload limits, malformed/missing preconditions, duplicate Authorization headers,
  missing/invalid tokens, request concurrency and deadlines receive structured errors.
  The capacity test caught premature permit release after extraction; the guard now
  owns the permit through completion, and blocking preparation retains a clone through
  cancellation. The regression test remains enabled.
- Runtime configuration requires explicit bounded values, hides secret values in
  configuration errors and rejects privileged DB sessions. A real restricted session
  selects only labyrinth_api and cannot read application tables directly.
- New SQL functions deny PUBLIC execution; the internal authorizer is not callable
  by the runtime role. Earlier migration/admission race/rollback tests remain enabled.

The published-status test inserts deliberately synthetic revision metadata through
the migration owner to test the read contract. It does not execute a worker or prove
artifact publication. No hosted Supabase/Render connection is claimed by local tests.

## Commands

With an isolated pgmq-enabled PostgreSQL and `LABYRINTH_TEST_DATABASE_URL` configured:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

The backend CI workflow runs the API suite with `postgres-tests`; the core workflow
runs ordinary workspace checks. Missing database configuration under the feature is
an error, not a skipped test. See [database setup](database-admission.md).

## Limits

Login/signup/refresh and automated CLI credentials are not implemented. No cookie
sessions, wildcard CORS or development auth bypass exists. Logout does not introspect
already-issued access JWTs; expiry and current database membership/suspension define
access. Key revocation latency includes local/upstream cache behavior. See
[ADR 0005](../decisions/0005-authenticated-job-http.md) for exact semantics.

Only process liveness is exposed publicly. Production TLS ingress, identity project
configuration, pool/limit sizing and network connectivity require deployment checks.
Worker leases/fencing/execution, cancellation APIs, durable error reports, staging,
publication/downloads, retention and admin operations remain unfinished milestones.

## Executed acceptance — 2026-11-15

Local PostgreSQL 18.6 with pgmq 1.8.1, Rust 1.95.0, locked dependencies. All commands
above passed: **158 tests**, zero failed or ignored, including 20 new auth/HTTP/runtime
checks and the previous 138 checks. The full run includes five compile-fail contracts
and the 126-device/496-failure-case engine regression. jsonwebtoken 10.4.0 and Axum
0.8.9 are locked in Cargo.lock. No real private signing key or hosted credential was
used or committed; test keys are generated in memory.

The API template lives in `api/.env.example`; the pre-existing untracked root
`.env.example` was left untouched. CI is configured to run these feature-gated API
checks with real PostgreSQL/pgmq; this local result does not assert a remote CI run.
