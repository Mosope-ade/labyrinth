# Project creation V1

`POST /v1/tenants/{tenant}/projects` requires the existing authenticated bearer token,
current active application user/tenant, owner/editor membership, and Idempotency-Key
(1–128 ASCII letters/digits or ._:-). Body: `{ "name": "Engineering design" }`.
Names contain 1–200 Unicode characters, no control characters or surrounding
whitespace. Unknown fields reject; body is capped at 8 KiB. Names need not be unique.

New creation returns 201; exact replay returns 200. Both return project_id, name,
head_revision_id (initially null), created_at and replayed. Location points to
`GET /v1/tenants/{tenant}/projects/{project}`, which permits current tenant members,
including viewers. Clients never choose the tenant owner or project ID in a body.

Idempotency keys are scoped to tenant/actor/create-project. Same key with a different
name returns 409 idempotency_conflict. Authorization is rechecked even on replay.
The tenant row serializes distinct and duplicate creations: quota checking, project
insert and immutable replay record commit together. Replays consume no additional
quota. Empty projects contain no generated revision or artifact.

The project cap is an explicit server policy, enforced transactionally for each
request. Configure LABYRINTH_TENANT_MAX_PROJECTS (1–10000); if absent, project creation
is unavailable (503) while existing read/generation routes remain available. Malformed
configuration prevents startup. No production value is inferred from a test fixture.
The approved initial project cap is 50. New personal tenants persist it; legacy
tenants without a persisted cap use the explicit deployment policy. See
[auth/onboarding V1](auth-onboarding-v1.md). No project request silently provisions
a tenant or membership.
