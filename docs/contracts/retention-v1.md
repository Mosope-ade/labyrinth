# Retention and project deletion V1

Approved policy (2026-11-16): published designs remain until owner deletion;
unsuccessful job inputs/evidence expire after 30 days; explicit project deletion has
seven days of recovery; application-managed backup content expires within 30 days.
Durations use fixed elapsed 24-hour days, independent of database timezone.
The 24-hour unpublished orphan minimum and 365-day audit minimum are unchanged.

## Owner API

`POST /v1/tenants/{tenant}/projects/{project}/delete` and the corresponding `/restore`
require an authenticated, enabled tenant **owner**, JSON Content-Type, and the strict
body `{ "confirm_project": "matching project UUID" }` (maximum 1 KiB). Editors,
viewers and unrelated admins cannot delete or restore. A mismatched confirmation is
400; denied access 403; unknown scoped project 404; expired recovery 409 with code
`recovery_expired`. Mutations and their metadata-only audit events commit together.

Both return 200 with `{project_id,state,deleted_at,purge_after}`. State is active,
deleted or purged; dates are UTC and nullable for active projects. Repeated deletion
does not reset the deadline or epoch; repeated restoration of an active project is a
no-op. Recovery is allowed strictly before deleted_at + 7 days, even if physical
cleanup is delayed. Restoring never resumes pre-deletion jobs.

`GET /v1/tenants/{tenant}/projects/deleted` is owner-only, with the normal UUID `after`
and limit 1–100/default 25 pagination. It returns project_id, name, deleted_at and
purge_after for deleted projects not yet fully purged, including overdue cleanup.
Ordinary project discovery excludes deleted projects. Normal project/job/revision/
artifact/evidence access and new admission/replays return 404 while deleted. Shared
tenant catalog discovery is unaffected. Already-delivered bytes cannot be recalled;
a read authorized before deletion may finish within its existing request bounds.

Deletion increments a lifecycle epoch in the project transaction. Jobs/attempts
capture their creation epoch. Worker claim, context, heartbeat and publication reject
deleted projects and retired epochs; restoring increments again. Thus a stale
worker cannot publish after deletion or after restoration. Up to 100 live jobs are
physically cancelled per transaction; remaining jobs are logically fenced immediately
and cancelled in bounded worker sweeps. Queue acknowledgments happen separately in
bounded batches with queue-before-job locking preserved. Process termination follows
the existing heartbeat/deadline bounds; storage writes cannot become published.

## Scheduled primary-data erasure

The existing worker runs serial retention passes alongside orphan cleanup. Each pass
retires/acknowledges at most 100 jobs/messages, erases at most 100 due job payloads,
and deletes one attempt's fixed 13-object inventory under a 60-second claim lease.
At most 16 passes run per tick, with timeouts and missed ticks skipped. No storage
I/O holds a database transaction. Partial deletion, crashes and stale claims retry.
The free deployment's local worker must be online: dates define eligibility, not a
guaranteed completion time while execution/storage is unavailable.

After 30 days from unsuccessful terminal completion, requirements, provenance,
options, raw idempotency keys and structured engine evidence are erased. Cancelled
jobs count as unsuccessful. Historical jobs without a recorded completion time start
a fresh 30-day window at migration, rather than assuming their creation time was the
end. Successful jobs and their reproducibility inputs never age out automatically.

After explicit deletion's seven-day deadline, the worker erases project payloads and
all attempt inventories, **including published packages belonging to that project**.
Other projects, shared immutable catalogs and audit records are excluded. This is a
separate path from orphan cleanup, which still never deletes published artifacts.
After payload and object passes complete, project/creation names are redacted and
the project is marked purged. Recoverable projects retain quota slots; purged ones
release them. Admin storage counts conservatively exclude fully purged projects.

Minimal project/job/attempt IDs, lifecycle/fencing history, catalog identities,
digests, idempotency fingerprints and deletion inventories remain as tombstones.
Raw keys are replaced by SHA-256 fingerprints so old keys cannot recreate a purged
project or replay as a new job. Revisions/artifact identity records remain immutable,
but their private contents are erased. This is permanent **content** erasure, not an
account-erasure API. No core assurance is inferred from erased payloads.

Successful deletion schedules repeat inventory sweeps after 24 hours to remove late
stale uploads; a partial failure retries after 60 seconds. Immutable guards permit
only owner-executed, policy-due payload/key/name erasure through restricted functions;
API/worker roles receive no direct table mutation or arbitrary-prefix deletion.

## Backups and recovery

Application-managed content backups must expire no later than 30 days, including
copies, replicas and exports. For a deleted project, use deleted_at + 30 days as the
outer deadline; creating an archive during recovery must not extend that deadline.
Failed-job data erased from the primary may remain only within this separate backup
window. Minimal lifecycle ledgers and mandatory audit history are not topology
content archives and retain their protective purpose.

Before opening a restored system, reapply the latest complete independent lifecycle
ledger using recovery-only functions. The ledger includes restores as well as
deletions, carries epochs and contains no design/name/input payload. Reconciliation
is monotonic, fences restored in-flight work and forces deleted-object cleanup to
run again, even when an old backup recorded cleanup as complete. It is not a public
HTTP or admin capability. If the latest ledger cannot be verified, keep the restored
system closed. See the [runbook](../runbooks/retention-and-recovery.md).

No live backup automation/provider guarantee is claimed here. Supabase database
backups exclude Storage object bytes, and Free projects need an independently
verified export/restore arrangement. Provider-managed expiry must be verified
separately before promising a deletion deadline. Sources:
[database backups](https://supabase.com/docs/guides/platform/backups),
[object deletion](https://supabase.com/docs/guides/storage/management/delete-objects).
