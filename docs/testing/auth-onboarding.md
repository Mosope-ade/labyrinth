# Authentication onboarding acceptance

Run the locked workspace/all-features suite, formatting and Clippy against the
isolated PostgreSQL/pgmq fixture. Tests use real ephemeral asymmetric JWT signatures
and the production JWT verifier with a test JWKS endpoint; no email is sent.

The signed-request/PostgreSQL tests prove:

- GET /v1/me does not insert an application user before explicit onboarding.
- Concurrent onboarding produces one personal tenant, one owner mapping and a
  replay response; new quotas are two active jobs and 50 persisted projects.
- Replay retains changed existing job quotas and never promotes a demoted member.
- Revoked membership, suspended tenant and suspended user cannot bypass access by
  replaying onboarding or creating another personal workspace.
- Actor, role, tenant and quota fields in the body reject without inserting a user.
- Missing onboarding configuration is unavailable; private tables remain unreadable
  to the API role.
- A personal tenant's 50-project cap remains authoritative even when a later router
  supplies a larger deployment fallback; creation at the cap rejects transactionally.

Project creation concurrency and idempotency are covered separately in
[project creation tests](project-creation.md). Existing auth tests cover signature,
issuer/audience/session/expiry/anonymous checks and bounded JWKS rotation/outage.

[Provider setup and hosted acceptance](../runbooks/auth-onboarding.md) still require
real SMTP/magic-link delivery, callback/verifier exchange, refresh/logout and client
credential-storage checks. These are not simulated by the local JWT fixture, and no
GUI or CLI login implementation is claimed by this backend checkpoint.

Acceptance checkpoint: locked workspace/all-features suite passed **182 tests,
0 failed, 0 ignored**. Formatting and workspace Clippy with warnings denied passed.
