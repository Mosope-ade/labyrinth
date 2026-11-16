# Admin operations V1

The user approved operator-only admin role grants/revocations, no implicit topology
access, a 365-day audit minimum, and one immutable catalog hash per semantic version.
Application JWT metadata never grants admin. Operators provision application users
normally, then maintain the private admin-role registry using separate credentials.
No role-grant/revoke HTTP endpoint is provided.

Every /v1/admin route requires current, unsuspended application-user membership in
that registry. Mutations recheck authority inside their database transaction. The
existing resource routes still require ordinary tenant membership: global admin
does not bypass private project, intent, artifact or job authorization.

Operations:

- GET /v1/admin/overview: enabled-user and tenant/project counts, job lifecycle counts,
  classified attempt counts, measured completed-attempt duration and artifact bytes.
  AI/provider/token-cost telemetry is explicitly unavailable until AI is implemented.
- GET /v1/admin/users and /v1/admin/tenants: bounded UUID pages of operational identity,
  suspension and quota metadata; no emails, raw prompts or topology contents.
- GET /v1/admin/audit?limit=25&before={decimal_id}: newest-first bounded audit records.
  IDs/cursors are strings; no event payload or secrets are copied into this log.
- POST /v1/admin/catalogs/validate: {yaml}; strict YAML/schema/semantic validation only.
- POST /v1/admin/catalogs: {yaml, expected_sha256, confirm_sha256}; validate and promote
  only the exact reviewed hash. A version already bound to another hash conflicts.
- POST /v1/admin/catalogs/{hash}/admission: {expected_admitted, admitted,
  confirm_sha256}; compare-and-set admission eligibility without changing snapshots.
- POST /v1/admin/users/{id}/suspension and /v1/admin/tenants/{id}/suspension:
  {expected_suspended, suspended, confirm_target}; compare-and-set. Admin accounts
  cannot be suspended through these routes; operator intervention is required.

Sensitive actions append actor/action/target/outcome/time in the same transaction as
mutations. Denied mutations return a result rather than rolling back their denial
record. Existing provisioned users denied by the admin extractor also receive an
audit denial; unknown application subjects have request telemetry only, preserving
side-effect-free identity inspection. Validation failures log only categories/hash,
never YAML/exception contents. Runtime roles cannot update/delete audit records.

A separate maintenance-only retention function/tool may prune bounded batches older
than 365 days. It never touches project/revision/job/artifact data. No automatic
retention is applied to published artifacts or attempt metadata by this contract.

Operational job intervention uses GET /v1/admin/tenants/{tenant}/projects/{project}/jobs/{job}
for state/attempt-count metadata and POST on its /cancel child with
{expected_state, confirm_job}. It follows normal cancellation/fencing semantics,
logs its action, and never reads inputs, recreates work or rewrites terminal history.
Suspension blocks interactive access/new admission; already accepted jobs are not
implicitly cancelled. Operators can use the explicit cancellation action when needed.

Catalog promotion replay does not re-enable an archived catalog. Re-enabling requires
the admission compare-and-set operation with the exact reviewed hash. Failed/not-found
mutations and denied registry access are audited; malformed HTTP payloads are covered
by request telemetry. Operator role grants/revokes and retention runs are also audited
with database session principal rather than a self-asserted application actor.
