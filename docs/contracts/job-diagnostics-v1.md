# Job attempt diagnostics V1

`GET /v1/tenants/{tenant}/projects/{project}/jobs/{job}/attempts` uses the same current
user/tenant/project membership authorization as job status. Viewers may read; outside,
suspended or revoked membership cannot. Unknown scoped jobs return 404.

The response is `{ "job_id": "uuid", "items": [...] }`, ordered by attempt number.
At most three attempts are exposed, matching the durable worker attempt budget. Each
item has attempt_id, attempt_number, started_at, finished_at, retry_scheduled, retry_at,
and diagnostic (nullable while running). Timestamps are UTC; legacy start times are
null because the previous schema did not record them. Retry time is returned only
for the current retry-wait attempt, never as a promise after retry exhaustion.

Diagnostics are fixed public categories/messages, not worker exception strings:
invalid_input, engine_rejected, resource_limit, storage_unavailable,
database_unavailable, process_io, lease_expired, stale_head, cancelled, succeeded,
or legacy attempt_failed. No worker ID, lease/fencing token, private prefix, stack
trace, raw intent, object key or credential is returned.

Resource exhaustion and expired execution leases have assurance outcome inconclusive.
A generic engine rejection has outcome rejected and does not assert proven network
infeasibility. Infrastructure failure has outcome unavailable. A stale project head
is conflict; cancellation is cancelled; a successful attempt is verified. These
assurance outcomes supplement existing job lifecycle states: job state failed means
execution ended unsuccessfully, not that the requested design is mathematically
infeasible. Bounded core counterexample transport is described below.

Worker failure classification and attempt finalization commit together under the
current unexpired fence. Public messages are generated from an allowlisted code;
client/worker-provided exception text cannot enter the diagnostics table/API.
Only the current attempt may finalize, and terminal classification remains immutable.
Lease expiry, cancellation and publication conflicts use their existing durable exit
classifications. Acknowledgment/retry semantics remain unchanged.

Diagnostics add no raw sensitive artifact store. Attempt metadata retention remains
part of the still-open overall job/history retention policy.

## Private structured engine evidence

`GET /v1/tenants/{tenant}/projects/{project}/jobs/{job}/attempts/{attempt}/evidence`
returns `{attempt_id, evidence}`. This is a project-content endpoint: current viewers,
editors and owners may read; an operator-granted admin without project membership
may not. Unknown scoped attempts return 404. Responses use no-store. Attempt listing
continues to return only its existing fixed, redacted diagnostic messages.

`evidence` is null while running, for legacy attempts, process/infrastructure failures
without a structured engine report, and when cancellation wins finalization. When
present it has:

- `schema_version`: `engine_failure_v1`.
- `outcome`: `rejected` or `inconclusive`; resource/search-budget exhaustion remains
  inconclusive, not proof of network infeasibility.
- `truncated`: whether the structured error was omitted because it exceeded the cap.
- `error`: the engine's existing GenerationError JSON, or null when omitted.

The engine runs without credentials. No stderr, worker exception, database error,
lease token, storage key or process environment is copied into this envelope. Core
counterexamples can include private site, zone, flow and addressing identifiers;
handle them as topology content, not anonymized operational telemetry. Top-level
serialization/configuration implementation messages are omitted rather than exported.

The child retains at most 16 KiB of compact structured error JSON. Larger reports
produce `truncated:true,error:null`; no partial JSON or silently incomplete proof is
returned. The parent accepts at most 32 KiB of failure-envelope wire JSON. PostgreSQL
independently caps its JSONB text representation at 64 KiB. Existing overall process
output/time/memory limits still apply. Resource kills may have no report at all.

Migration 0015 stores evidence on the attempt, atomically with existing finalization
and diagnostic classification under the current unexpired fence. Stale/replayed
finalizers cannot replace it. Cancellation suppresses evidence; successful published
revisions are unchanged. Evidence is immutable once recorded and unavailable through
admin metadata routes. Runtime roles cannot access the table directly; the worker
can execute only the write function and the API only the membership-scoped reader.

Deploy migrations before replacing API/worker binaries. Readiness now requires the
new reader function. Old workers can still finalize through the previous diagnostic
function, yielding null evidence; old API instances continue serving the existing
attempt list. This additive rollout introduces no published-artifact cleanup and no
new retention/deletion policy: evidence currently follows attempt history retention.

Acceptance evidence and commands: [engine evidence tests](../testing/engine-evidence.md).

## Retention extension — 2026-11-16

Migration 0016 adds the approved policy-bound erasure exception: evidence from
unsuccessful jobs expires after 30 days, and explicit project deletion erases content
after its seven-day recovery window. The evidence endpoint returns null after
erasure; fixed diagnostic categories and minimal fencing metadata remain. See
[retention V1](retention-v1.md). Fresh evidence is still immutable and cannot be
replaced by a stale finalizer. No arbitrary exception strings enter public responses.
