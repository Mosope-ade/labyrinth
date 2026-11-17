# Hosted worker contract V1

See [ADR 0006](../decisions/0006-fenced-worker-publication.md) for decisions and
[acceptance evidence](../testing/worker-publication.md). No additional public HTTP
endpoint or synchronous generation path is introduced.

## Runtime

`cargo run -p labyrinth-worker --locked` starts one sequential consumer. Use
`worker/.env.example` as the complete required configuration list. Build and run on
Linux with util-linux `prlimit` installed. Configure a verified TLS PostgreSQL URL
and a dedicated, nonadministrative login granted `labyrinth_worker`. Apply migrations
with the privileged migration tool first; the worker never migrates or provisions.
Startup checks bucket metadata and refuses public buckets. Provision the bucket
privately with JSON uploads enabled and limits consistent with worker output limits.
Supabase storage origin is an HTTPS origin only; redirects and path credentials
are forbidden. Upload/download requests have 3-second connect and 20-second total
limits; the entire attempt still has its own 120-second default deadline.

SIGTERM/Ctrl-C drops active work, killing the child; lease expiry/redelivery recovers
the attempt. One consumer per process is intentional. Increase replicas only after
measuring database, parent memory, engine capacity and storage throughput.

## Database calls

- `claim_job(worker, lease_seconds, attempt_budget, first_delay, second_delay)` returns
  SQL NULL when idle/recovering, otherwise `{tenant_id, job_id, fence}`.
- `attempt_context(tenant, job, fence)` returns immutable requirements, stable-ID
  provenance, options, catalog YAML/hash, base revision/digest, authorized base object
  inventory and the attempt prefix. An invalid/expired fence returns SQL NULL.
- `heartbeat_attempt(tenant, job, fence, lease_seconds)` atomically renews the lease
  and message visibility; false means stop work immediately.
- `finish_attempt(tenant, job, fence, transient, budget, delays...)` marks a durable
  terminal result or retry schedule. False means ownership was lost.
- `publish_attempt(tenant, job, fence, revision, manifest_hash, inventory)` atomically
  publishes metadata only if all concurrency preconditions hold. False includes a
  stale fence/cancelled job or a stale head (the latter is recorded as job failure).

Inventory entries are `{name, object_key, sha256, size_bytes}`. Exactly 13 unique
core artifact names are accepted, totaling at most 32 MiB. Every object key must
be exactly the server-issued attempt prefix plus artifact name. This is a trusted
worker interface: SQL validates metadata constraints; worker code proves the bytes
were generated/checked/uploaded. Giving a malicious process this role grants it
publication authority, so never share it with user-facing services.

## Child protocol and package checks

The child receives one `EngineInput` JSON object: immutable context and optional
base file map. All serialized input, including the base package, must fit the
configured 2 MiB limit; larger incremental inputs currently fail explicitly.
File byte maps use JSON byte arrays. Parent wire output is bounded to four times
the decoded package budget; package validation enforces the decoded 32 MiB limit.
Child exit 0 returns the file map; exit 2 means deterministic rejection. Signals,
nonzero resource exits and wall-time exhaustion cannot publish a partial package.

Canonical saved requirements are revalidated and their saved provenance restored;
missing/extra/unknown provenance entries reject. The parent cross-checks the output
against this input and authoritative base manifest metadata before storage calls.
Successful upload is insufficient: every object is read back and checked before
publication. Objects remain private under their original attempt keys after success.

Failure classifications are persisted on attempts (`lease_expired`,
`transient_failure`, `deterministic_failure`, `stale_head`, `cancelled`, `succeeded`).
Public status currently exposes job state/revision only. Resource failures are
terminal and logged by category; richer persistent diagnostics remain pending.

## Orphan cleanup extension — 2026-11-16

Workers now require LABYRINTH_ORPHAN_SAFETY_SECS (initial value 86400). A bounded
background task removes only old unsuccessful, unreferenced attempt inventories;
published artifacts are preserved. See [cleanup V1](orphan-cleanup-v1.md) and
[acceptance](../testing/orphan-cleanup.md). Earlier deferred-cleanup notes are
superseded by this extension; published revision retention remains undecided.

## Diagnostic reporting extension — 2026-11-16

Workers now finalize failures through finish_attempt_report, persisting an allowlisted
public category in the same transaction as retry/terminal state. Child exit code 3
preserves resource exhaustion (including cooperative search/verification budgets)
across process boundaries; exit 2 remains generic rejection. No raw exception text
is persisted. See [diagnostic contract](job-diagnostics-v1.md). Earlier worker
finalization functions remain for rolling compatibility and legacy attempts.

## Retention extension — 2026-11-16

Workers now use finish_attempt_evidence for bounded private engine reports and run
policy-bound retention alongside orphan cleanup. Migration 0016 wraps claim/context/
heartbeat/publication with project lifecycle epochs. Ownership is rechecked before
each artifact write. Explicit owner deletion can erase that project's published
content after seven days; orphan cleanup still cannot. Unsuccessful payload/evidence
retention is 30 days. See [retention V1](retention-v1.md) and
[acceptance](../testing/retention.md). These rules supersede earlier deferred-policy
notes; backup configuration and live restore acceptance remain separate.

## Technical Storage identity extension — 2026-11-17

Replace LABYRINTH_STORAGE_TOKEN with explicit API_KEY, EMAIL and PASSWORD settings.
The worker obtains/refreshes a dedicated technical session with a publishable key;
no raw user token or service-role key is a runtime fallback. Provider RLS additionally
limits live writes and policy-eligible cleanup. The engine child receives no identity
credentials. Apply the reviewed provider bootstrap after application migration 0016;
see [setup](../../SUPABASE_SETUP.md). No additional application migration is required.
