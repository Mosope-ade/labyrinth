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

Production deployment/load testing, authorized downloads, orphan retention/cleanup
and public cancellation/diagnostics remain separate work. No live service is
provisioned by this repository change.
