# ADR 0007 — Explicit personal workspace onboarding and interactive authentication

Status: accepted, 2026-11-16.

The user selected Supabase email magic-link login with PKCE, explicit authenticated
personal-workspace onboarding, and initial tenant quotas of two active generation
jobs and 50 projects. Signing in alone must not provision application resources.

Supabase handles email verification/link delivery, code exchange, refresh rotation
and logout. GUI and CLI auth clients implement those flows in their own milestones;
the Rust resource API continues verifying access JWTs and current database access.
No password or refresh-token proxy endpoints are introduced. Current application
suspension is immediate even though access-token logout revocation is bounded by
JWT expiry. Scripted credentials remain a separate future contract.

An explicit onboarding transaction serializes on the authenticated subject and creates
at most one immutable user-to-personal-tenant mapping. Existing users may also choose
to create their personal workspace explicitly. Replay never restores membership or
promotes a role, and cannot evade a suspended tenant by creating another personal
workspace. No email, provider metadata, password or refresh token is copied into the
application database. No admin role is issued.

New personal tenants persist max_active_jobs=2 and max_projects=50 from explicit
validated deployment configuration. Replay does not rewrite those values. Existing
tenants retain their active-job quotas; legacy project caps use the explicit server
policy when none is persisted. Project creation serializes on the tenant row and
uses immutable actor-scoped replay records to prevent duplicate admission.

Consequences: provider setup and real email/callback verification are hosted release
gates. Backend tests use real asymmetric signed tokens and PostgreSQL but do not
claim live magic-link delivery or a completed GUI/CLI login interface. Client token
storage, browser security/CORS and CLI OS-keyring integration need implementation
and acceptance in those client milestones.

See [auth/onboarding contract](../contracts/auth-onboarding-v1.md),
[project creation](../contracts/project-creation-v1.md) and
[Supabase PKCE documentation](https://supabase.com/docs/guides/auth/sessions/pkce-flow).
