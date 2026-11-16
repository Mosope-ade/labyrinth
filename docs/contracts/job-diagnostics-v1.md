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
infeasible. Full core counterexample transport remains a later diagnostic extension.

Worker failure classification and attempt finalization commit together under the
current unexpired fence. Public messages are generated from an allowlisted code;
client/worker-provided exception text cannot enter the diagnostics table/API.
Only the current attempt may finalize, and terminal classification remains immutable.
Lease expiry, cancellation and publication conflicts use their existing durable exit
classifications. Acknowledgment/retry semantics remain unchanged.

Diagnostics add no raw sensitive artifact store. Attempt metadata retention remains
part of the still-open overall job/history retention policy.
