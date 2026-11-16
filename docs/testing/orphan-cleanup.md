# Orphan cleanup acceptance

Use the same isolated real PostgreSQL/pgmq setup and locked workspace commands as
[worker acceptance](worker-publication.md). The suite uses no hosted credentials.

The hosted worker integration test exercises real admission, supervised generation,
publication and incremental update before cleanup. It snapshots every published
object, then checks:

- No young failed attempt is cleaned; windows below 24 hours reject.
- Concurrent claims belong to distinct attempts; expired cleanup tokens cannot check
  ownership. The worker role alone can claim; API function execution is denied.
- Partial deletion returns failure, survives durable retry scheduling and completes
  on replay. Missing objects do not prevent retry success.
- Every published object remains byte-identical, including historical revisions.
- A late upload under a cleaned failed prefix is removed by a subsequent sweep.
- Cleanup runs concurrently with a fresh real generation/publication; that revision
  succeeds while historical published bytes remain unchanged. A stale failed fence
  cannot publish, and resetting a terminal attempt classification rejects.
- Attempting to attach a new artifact reference to a failed prefix rejects with the
  explicit terminal_attempt_cannot_publish trigger error.

A storage adapter HTTP test checks the exact deletion endpoint, authorization
headers and full-key request body, failure propagation, repeated success, and early
rejection of mixed prefixes/duplicate keys. Its insecure loopback URL override exists
only inside cfg(test); production configuration still requires HTTPS.

Migration checksums for existing migrations remain untouched. The new migration
records conservative timestamps for legacy terminal attempts. No test weakens the
existing immutable revision or artifact constraints. Production Supabase deletion,
provider outage/chaos testing and measured cleanup throughput remain unverified.

Acceptance checkpoint (2026-11-16): the locked workspace/all-features suite passed
**174 tests, 0 failed, 0 ignored**, including real PostgreSQL 18.6/pgmq 1.8.1 and
supervised engine execution. Formatting and workspace Clippy with warnings denied
passed. Existing migration files and tests were preserved.
