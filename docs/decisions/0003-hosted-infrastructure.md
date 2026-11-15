# ADR 0003 — Hosted infrastructure

Date: 2026-11-15
Status: Accepted provider direction; provisioning and deployment not performed.

## Context

The verified pure engine must support one hosted system shared by separate CLI and
GUI clients. Generation is asynchronous, tenant-scoped and independent of AI.
The user selected Cloudflare for the frontend, Render for backend/engine execution,
Supabase for database/authentication and GitHub Releases for CLI packages, then
instructed continuation with the recommended integrated storage/queue design.

## Decision

- Cloudflare Pages hosts the React/TypeScript frontend.
- Render runs separate Docker services: public Axum API and background worker.
  The Rust engine is bundled with the worker and runs in a supervised child process;
  it has no public generation endpoint of its own.
- Supabase Postgres stores tenant memberships, projects, jobs, attempts, revisions,
  artifact metadata and audit records. Repository-owned migrations define the schema.
- Supabase Auth establishes user identity. Axum validates asymmetric JWTs using JWKS
  and independently enforces membership, suspension and admin authorization.
- Private Supabase Storage buckets hold immutable engineering packages and staged
  attempt artifacts. Downloads require backend authorization and short-lived URLs.
- Supabase Queues (pgmq) provides durable generation dispatch. The application
  assumes redelivery and uses leases, fencing and idempotent publication.
- GitHub Releases distributes versioned CLI binaries with checksums and attestations;
  all assets are uploaded before immutable publication.

Start with one API and one worker, one generation process per worker. Scale only
within measured CPU/memory and database connection budgets. Use geographically close
Render/Supabase regions after measuring connectivity; exact regions and service sizes
remain deployment decisions. Separate development/staging/production credentials/data.

## Consequences

Using Supabase Storage/Queues avoids separate storage and Redis integrations in V1.
It couples queue throughput to database resources, which must be monitored and bounded.
Storage/queue adapters keep provider details outside engine contracts. R2 or a dedicated
queue can be evaluated if measured requirements warrant migration.

No transaction spans Postgres and object storage. Upload into attempt-specific staging,
validate every hash/size, then atomically publish revision metadata in Postgres under a
current lease/fencing token. A failed or stale attempt cannot publish. Orphan cleanup
is retryable and must not delete published artifacts. Queue acknowledgment follows the
durable terminal transition; duplicate dispatch cannot duplicate revisions.

Restrict database roles and use RLS where applicable; privileged connections must not
be assumed to provide tenant isolation automatically. Clients do not directly mutate
application or queue tables. Never ship database/storage secrets in frontend or CLI.
CLI browser login uses PKCE; scripted credentials require a separate scoped/revocable
contract. Hard process resource limits, quotas, recovery and retention still need
implementation and tests before hosted acceptance.

## References

- [Render workers](https://render.com/docs/background-workers)
- [Supabase connections](https://supabase.com/docs/guides/database/connecting-to-postgres)
- [Supabase signing keys](https://supabase.com/docs/guides/auth/signing-keys)
- [Private buckets](https://supabase.com/docs/guides/storage/buckets/fundamentals)
- [Supabase Queues](https://supabase.com/docs/guides/queues)
- [Cloudflare React](https://developers.cloudflare.com/pages/framework-guides/deploy-a-react-site/)
- [GitHub immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases)
