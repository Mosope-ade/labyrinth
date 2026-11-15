# ADR-0004: Transactional database admission

## Status
Accepted for the first hosted persistence slice; worker/publication implementation follows.

## Date
2026-11-15

## Context

A successful HTTP retry must not create duplicate work. Concurrent requests must not
exceed a tenant quota or bind an incremental design to another project's revision.
The queue and application data share Supabase Postgres, allowing atomic admission.
A pooler means session-local identity variables must not become authorization state.

## Decision

Use SQLx migrations in `api/migrations/`, applied explicitly by a migration owner.
Runtime connections use a dedicated login limited to the NOLOGIN `labyrinth_api`
group role. That role receives only schema usage and EXECUTE on the admission
function; no direct application table, queue or admin mutation rights.

Keep application tables in the private `labyrinth` schema with RLS enabled and no
client policies/grants. A narrowly scoped SECURITY DEFINER function with fixed
`pg_catalog` search path uses fully qualified objects. The migration owner is trusted;
it is never the runtime login. Supabase public clients cannot invoke admission.
The verified actor ID is supplied by the trusted API, not an HTTP body or SQL setting.

Serialize admission per tenant using its row lock. Lock order is user, tenant,
membership, project, catalog. Membership/user locks prevent admission racing past a
committed suspension/revocation. Idempotency replay still requires current access.
New requests compare the explicit project head, resolve the admitted catalog/base,
check the configured active-job quota, insert the job and call `pgmq.send` in one
statement transaction. A queue failure rolls back the job and idempotency key.
The per-tenant serialization is deliberate: optimize only after measuring contention.

Idempotency keys are scoped to tenant/actor/generate_v1, retained with the immutable
job. Effective normalized requirements, identity-based provenance, options, catalog,
project and head precondition enter a versioned request digest. The key itself does
not enter that digest. Replays return the original current job state, without
requeueing, including after the project head has advanced. Different payloads conflict.

Database revisions use UUIDs. Core revisions use the exact `rev-<canonical UUID>`
representation because the core's stable identifiers must start with a letter.
The stored base manifest digest is resolved from authorized revision metadata, not
accepted from the submission. Full saved-package and pin revalidation remains mandatory
in the worker; admission readiness is not engineering verification.

## Consequences

No passwords, emails or access tokens are duplicated in application user rows;
`users.id` is the Supabase subject. Explicit provisioning must establish users,
tenants, memberships, quota and catalog snapshots. There is no automatic admin grant,
public signup-to-tenant policy or default production quota in this slice.

Composite tenant/project foreign keys protect relationships even in maintenance code.
Versioned catalogs allow admission toggling without changing historical YAML/hash.
Revisions/artifacts/audit rows reject UPDATE/DELETE; later retention needs a reviewed
privileged deletion path. Worker state transitions and publication privileges are not
provided by this admission function.

On an unknown database commit outcome, clients retry the same idempotency key.
Connectivity/deadlock/lock-timeout failures are not reported as success. Connection,
statement and lock timeouts and pool sizes must be explicitly configured by deployment.
Use transaction-safe SQL and avoid session authorization state with pooled connections.

## References

- [PostgreSQL row security](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)
- [PostgreSQL schema trust](https://www.postgresql.org/docs/current/ddl-schemas.html)
- [Supabase pgmq](https://supabase.com/docs/guides/queues/pgmq)
- [SQLx migrations](https://docs.rs/sqlx/0.8.6/sqlx/migrate/index.html)

ADR 0005 extends this function-only runtime interface with authorized catalog/status
reads. Direct runtime table grants remain absent.
