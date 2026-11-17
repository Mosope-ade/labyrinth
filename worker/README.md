# Generation worker

A separately runnable, sequential worker for durable admitted generation jobs.
Uses the pure core in a bounded Linux child process, renews PostgreSQL/pgmq leases,
and stages/verifies private Supabase Storage objects before atomic publication.

Read the [worker contract](../docs/contracts/worker-v1.md),
[decision](../docs/decisions/0006-fenced-worker-publication.md) and
[test evidence](../docs/testing/worker-publication.md).

Install util-linux (`/usr/bin/prlimit`), build with `cargo build -p labyrinth-worker
--locked`, apply API migrations using the migration owner, and configure every
variable in [the environment template](.env.example). Supply secrets through your
runtime secret manager. The worker uses no dotenv autoload and does not provision
roles, buckets or schema. Start with `cargo run -p labyrinth-worker --locked`.

Use a dedicated unprivileged database login granted `labyrinth_worker` and a private
storage bucket. Keep its database/storage credentials out of the API/frontend.
Hosted TCP verifies TLS. Missing configuration or a public/inaccessible bucket
prevents startup. No jobs are processed synchronously by the API.

Shutdown or lease loss kills active child execution; queue redelivery recovers it.
A failed/cancelled job publishes no partial package. Transient retries are bounded;
resource exhaustion and deterministic rejection are terminal. A saved base package
counts toward the 2 MiB serialized child-input limit. See the contract for limits.

Production deployment/load and live provider acceptance remain separate work.
Retention and private diagnostics are implemented; see their contracts. Authenticated downloads/cancellation are implemented
in the API. No live service is
provisioned by this repository change.

Orphan cleanup now runs alongside generation. Set LABYRINTH_ORPHAN_SAFETY_SECS=86400
and grant the server-only storage credential deletion permission for staging objects.
Published revisions are excluded. See [cleanup V1](../docs/contracts/orphan-cleanup-v1.md)
for the terminal-age window, retries, repeat sweeps and bounded batches. Earlier
pending-cleanup notes are superseded by this extension.

Storage now uses the reviewed technical identity/RLS bootstrap and automatic session
refresh. Follow [Supabase setup](../SUPABASE_SETUP.md); static TOKEN config is no
longer accepted. Human sign-in remains passwordless PKCE.
