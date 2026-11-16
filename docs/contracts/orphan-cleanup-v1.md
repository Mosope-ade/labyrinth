# Orphan attempt cleanup V1

The agreed initial safety window is **24 hours after an attempt becomes terminal**.
Set `LABYRINTH_ORPHAN_SAFETY_SECS=86400` explicitly on the worker. Missing, malformed
or shorter settings prevent startup. Longer conservative windows up to 30 days are
supported. This policy concerns unpublished staging objects only; published revision
retention, project deletion and backups follow the separate [retention policy](retention-v1.md).

Eligible attempts have an immutable unsuccessful terminal classification, an ended_at
older than the window, a due cleanup time, no live cleanup claim, and **no artifact
references anywhere under their private attempt prefix**. Running/unclassified and
successful attempts are excluded. Pre-migration terminal history starts a fresh
window at migration time because its true completion time was not stored.

Database claim/check/finish functions are available only to the worker role. Each
claim receives a fresh token and a 60-second cleanup lease; concurrent replicas use
SKIP LOCKED. Before external deletion the worker rechecks token, lease, terminal
classification and artifact references. Cleanup runs no storage I/O inside a database
transaction. A trigger prohibits any future artifact attachment to a failed attempt's
prefix, closing the reference-check/deletion race. Terminal classifications cannot
be reset to make a cleaned attempt publishable. Existing immutable revisions/artifacts
are never deleted or altered by cleanup.

The storage request deletes exactly the fixed 13 full object keys belonging to one
attempt. It does not recursively delete a prefix or empty a bucket. Unknown names,
mixed prefixes, duplicate keys and traversal paths reject. Use Supabase Storage's
object deletion API, not SQL against storage metadata. Requests use the worker's
server-only storage credential with the existing HTTPS/no-redirect/time bounds.
The credential must permit deletion of private staging objects; no new permission
is given to API, user or anonymous database roles.

Success records last_cleaned_at and schedules a repeat sweep in 24 hours. Repeat
sweeps handle objects uploaded late by an old, fenced worker; such a worker still
cannot publish. Missing objects are harmless. A failed or partial deletion schedules
retry after 60 seconds; a crash leaves the cleanup lease to expire. Cleanup resumes
using the same immutable full-key inventory, making retries idempotent. Attempt
metadata/tombstones are retained; their eventual retention is separate work.

Each worker has one serial cleanup task alongside its one generation task. It ticks
every 60 seconds, handles at most 16 attempts per tick, stops the batch on idle/error,
and skips missed ticks. No unbounded queue or overlapping delete calls are created.
SQL and storage deadlines still apply. Shutdown drops pending work; expired claims
recover without holding database connections across storage calls.

Operational errors emit only `orphan_cleanup_failed`; no credentials, object paths or
network requirements enter logs. Detailed cleanup metrics/admin presentation and
live hosted chaos/load acceptance remain follow-up work. This does not remove
unexpected/manual object names outside the engine's fixed inventory.

References: [Supabase object deletion](https://supabase.com/docs/guides/storage/management/delete-objects),
[storage schema limitations](https://supabase.com/docs/guides/storage/schema/design).
