# ADR 0006: Fenced worker execution and atomic artifact publication

Status: accepted, 2026-11-16. Extends ADRs 0004 and 0005.

The worker is a separate trusted process with a dedicated `labyrinth_worker`
NOLOGIN group role. Its login must be unprivileged and have permission to assume
that role. Neither the API nor browser/CLI receives this role or storage credentials.
Worker SQL functions use fixed `pg_catalog` search paths and explicit grants;
application tables and pgmq remain inaccessible directly to runtime roles.

The user approved these configurable initial settings: one job per worker process,
60-second lease, 15-second heartbeat, 120-second entire attempt deadline, 60 CPU
seconds and 1 GiB address-space ceiling for the engine child, 2 MiB serialized child
input and 32 MiB decoded package output. Three total attempts are allowed, with
10-second and then 60-second transient-failure delays. Environment settings are
required; missing/invalid settings prevent startup. These are initial safeguards,
not measured production SLOs. No external solver is introduced by this change;
the existing per-solver invocation requirement remains applicable if one is added.

Every claim inserts a fresh UUID fence and immutable attempt identity. Heartbeat,
context retrieval, failure finalization and publication require the current,
unexpired fence. Queue visibility and database lease renewal share a transaction.
Expired attempts recover on queue redelivery; recovery marks the old attempt and
schedules backoff before a replacement may claim. Attempt counts survive restarts.
No separate timer/reaper service is necessary for this queue-driven recovery path.

Network I/O and generation occur outside publication transactions. Worker mutations
lock an existing queue row before job rows to avoid queue/heartbeat lock inversion;
publication also follows admission's project-before-job order. The publication
function checks the fence, lease, running state, expected head and exact 13-file
inventory/prefix, then creates revision/artifact records, moves the project head,
marks success and deletes the queue message in one transaction. Thus there is no
commit-before-ack window for success. Repeated finalization cannot create another
revision. Duplicate terminal messages are discarded when read.

The parent runs its own executable in `--engine-child` mode via Linux `prlimit`.
It clears the child's environment, pipes bounded JSON, discards stderr, disables
core dumps and applies CPU/address-space/open-file limits before exec. Tokio
kill-on-drop covers lease loss, cancellation and shutdown; hard wall deadlines
kill and wait explicitly. The engine child executes no external programs and has
no storage/database credentials. This is a trusted computation boundary, not a
sandbox for executing arbitrary user code. OS/container limits must additionally
bound the complete parent process; address-space limits are not RSS quotas.

The worker validates package inventory, every digest/size, requirements/options,
catalog, provenance, base identity and five-pass report before uploading. Private,
attempt-specific objects are created without overwrite and read back for digest
confirmation. Only the metadata transaction publishes them. Supabase bucket
privacy is checked at startup. A private bucket and restricted worker credential
must be provisioned separately. The credential can read/write sensitive artifacts;
it must never enter API responses, client bundles, child environments or logs.

Deterministic rejection, malformed immutable context and resource exhaustion do
not automatically retry. Storage/process-I/O/database failures use the bounded
retry policy; a lost fence cannot finalize anything. Cancellation requested by
existing database state prevents publication and is completed by queue recovery.
The user-facing cancellation endpoint is still a separate milestone.

Deferred: orphan-object retention/cleanup (including publication-race safety),
authorized downloads, public diagnostics, deployment packaging/provisioning,
backup/restore drills and live provider smoke/load tests. Objects are never deleted
by this slice, so cleanup cannot accidentally remove published artifacts. These
remaining items prevent calling the whole hosted milestone complete.

References: [Tokio child lifecycle](https://docs.rs/tokio/latest/tokio/process/),
[Supabase private buckets](https://supabase.com/docs/guides/storage/buckets/fundamentals),
[authenticated downloads](https://supabase.com/docs/guides/storage/serving/downloads),
[non-overwriting uploads](https://supabase.com/docs/guides/storage/uploads/standard-uploads).
