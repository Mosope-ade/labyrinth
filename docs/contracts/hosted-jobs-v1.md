# Hosted job lifecycle — V1 implementation contract

Status: initial migrations and transactional admission implemented and tested.
Authenticated HTTP submission/status is implemented; worker leasing/execution,
storage publication and recovery remain pending.
Provider direction: [ADR 0003](../decisions/0003-hosted-infrastructure.md).

## Ownership and records

A tenant owns projects. Membership links an authenticated user to a tenant and its
role. Every project/job/revision query includes tenant scope from verified membership;
request-supplied tenant IDs are selectors, never authorization. Admin roles do not
implicitly grant access to private topology contents.

A job records immutable confirmed requirements, generation options, catalog identity,
input digest, optional base revision/digest, creator and project. Attempts record
lease token, worker identity, lease expiry, heartbeat, exit classification and staged
prefix. Revisions reference one successful job and a complete immutable manifest.
Artifact metadata records fixed filename, object key, bytes and SHA-256. Unique
constraints prevent multiple published revisions per job and duplicate artifact names.
All foreign-key relationships must preserve tenant/project scope.

## Admission

Validate schema/readiness and supported options before enqueueing. Resolve catalog
and base revision through authorized server metadata. Incremental input must bind the
expected immutable base digest. Reject stale project head using optimistic concurrency.

Require an idempotency key scoped to tenant, actor and operation. Store a normalized
request digest with the result. Same key/same digest returns the original job; same
key/different digest conflicts. Create the job and pgmq message in one database
transaction. Queue payload contains only job identity/version, not duplicated secrets
or mutable engineering requirements. Admission and quota checks are transactional.

## State transitions

`queued -> running -> succeeded | failed`

`running -> retry_wait -> queued` for bounded transient retries.

`queued | retry_wait -> cancelled`; `running -> cancel_requested -> cancelled`.

A cancellation racing successful publication is serialized by locking the job row:
publication can win only before cancellation is committed. A terminal job is immutable;
manual retry creates a linked new job rather than erasing its history.

Claiming a job creates an attempt with a fresh fencing token and expiry under a row
lock. Only the current unexpired attempt may heartbeat or finalize. Worker visibility
renewal and DB lease renewal are coordinated, but neither proves exclusive execution
after a crash. Stale workers cannot publish even if they continue computing.

## Execution and publication

A worker loads immutable authorized context, verifies hashes and runs the core in a
supervised child process. Bound input/output bytes, runtime, memory and concurrency;
terminate on cancellation, deadline or lease loss. Exact deployment limits are explicit
configuration validated at startup, not inferred from engine cooperative deadlines.

Stage output under an attempt-specific private prefix. Check the fixed inventory,
all hashes/sizes and manifest identity before publication. In a short DB transaction,
lock the job/project, verify current fence/lease and cancellation state, check expected
project head, create revision/artifact records, move project head and mark success.
No upload or core computation runs inside that transaction. Objects need not be moved:
private staged keys become referenced immutable revision objects atomically via metadata.
Only published revision records permit download authorization.

Acknowledge the queue message after durable terminal state or durable retry scheduling.
Redelivery of terminal jobs is acknowledged without re-execution. Transient retry uses
bounded backoff; deterministic input/infeasibility failures do not retry automatically.
Do not claim exactly-once execution; guarantee at most one published revision per job.

## Recovery and cleanup

A reaper recovers expired leases, fences old attempts and schedules retries or terminal
failure under the same job lock. Attempt budgets survive worker/API restarts. Staged
objects without published references become cleanup candidates after a safety window;
cleanup rechecks references and is idempotent. Database backups alone do not restore
artifact bytes: object retention/restore must be tested with revision metadata.

## Required acceptance tests before M4 completion

- Cross-tenant project, base revision, status and download access denies.
- Duplicate admission, mismatched idempotency payload and concurrent quota admission.
- Crash before/after enqueue, upload, DB publication and queue acknowledgment.
- Expired worker continues running while a replacement claims: only current fence wins.
- Cancellation races admission, execution and publication without partial visibility.
- Storage failure/hash mismatch never publishes; retry never creates duplicate revision.
- Stale project head conflicts; existing verified revision remains unchanged.
- Hard process deadline, cancellation termination and bounded retry exhaustion.
- Cleanup cannot remove published objects, including during concurrent publication.

Authentication flow, numeric quotas/timeouts/retention and admin role provisioning
need their own contracts before corresponding runtime behavior is implemented.

## Admission persistence slice — 2026-11-15

See [ADR 0004](../decisions/0004-transactional-admission.md) for the restricted
role/function boundary and [database test evidence](../testing/database-admission.md).
The admission library uses a server-authenticated subject; the JWT verifier and
routes are documented in [HTTP V1](http-jobs-v1.md). Hosted revision UUIDs map to core `rev-<canonical UUID>` IDs.
