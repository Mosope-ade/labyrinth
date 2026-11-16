# Worker execution and publication acceptance

Local tests use real PostgreSQL 18.6 and upstream pgmq 1.8.1; CI uses the existing
PostgreSQL 17 fixture with the same pinned extension. Tests requiring PostgreSQL
must receive an isolated privileged `LABYRINTH_TEST_DATABASE_URL`; they never skip
silently. Worker fixtures create/drop their own uniquely named databases to isolate
queue consumption from API admission tests. The test login therefore needs CREATEDB.

Run:

```sh
CARGO_HOME=$PWD/.local/cargo-cache cargo fmt --all --check
CARGO_HOME=$PWD/.local/cargo-cache cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
CARGO_HOME=$PWD/.local/cargo-cache LABYRINTH_TEST_DATABASE_URL=postgresql://USER@127.0.0.1:55441/ISOLATED_DB cargo test --workspace --all-features --locked
```

Evidence mapping:

- `api/tests/worker_leases.rs`: restricted worker/API grants, expired lease denial,
  durable recovery/backoff, changed fence, stale-worker publication denial,
  malformed inventory rollback, atomic 13-record publication, duplicate finalization,
  retry exhaustion, cancellation and competing expected-head publication.
  Metadata-only publication fixtures are deliberately synthetic; they do not claim
  to verify object storage or engine output.
- `worker/tests/execution.rs`: real supervised engine generation, package corruption,
  provenance round trip, invalid input, bounds validation, hard wall deadline,
  oversized output, dropped-future child termination/reaping, CPU and address-space
  limits. These tests require Linux, `/usr/bin/prlimit`, shell/sleep/yes and Python 3.
- `worker/src/storage.rs` tests: strict production HTTPS/path checks plus local HTTP
  request/response integration for authentication headers, non-overwrite uploads,
  authenticated private download paths, status errors and response bounds.
- `worker/tests/hosted.rs`: real admission/PostgreSQL/pgmq and real supervised engine,
  with an injected in-memory object store: failed upload -> durable retry -> verified
  publication -> saved-base incremental update; corrupt read-back never advances head.
  No production plain-HTTP switch or verification bypass is used.

Live Supabase/Render deployment, storage-role provisioning, crash/kill chaos tests
against those providers, orphan cleanup and restore drills remain unverified.

Acceptance checkpoint (2026-11-16): workspace/all-features suite passed **170 tests,
0 failed, 0 ignored**. Formatting, locked workspace checks and Clippy with warnings
denied passed. Storage adapter tests additionally exercise the startup private-bucket
metadata check. No tests were skipped or weakened for this slice.
