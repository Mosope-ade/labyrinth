# Structured engine failure evidence acceptance

Trust boundaries are credential-free engine child → supervised worker → fenced SQL
finalization → current project membership. Evidence can contain private network IDs.
It must never be returned through global admin metadata or emitted in process logs.

Coverage:

- Real child search-budget exhaustion returns structured GenerationError JSON and
  keeps assurance inconclusive. Existing success/package-integrity, process CPU/memory,
  deadline/output and cancellation-kill checks remain active.
- Oversized evidence becomes an explicit truncation marker; malformed/unknown-field
  envelopes and outcome/exit-code mismatches reject. Exceptions are not serialized.
- Real hosted worker execution persists evidence while failing the job without a
  published revision. Existing upload retry, incremental generation, publication
  fences and published-artifact-preserving orphan cleanup still run in that test.
- PostgreSQL rejects oversize reports before lifecycle mutation. Wrong/expired fences
  and repeated finalizers cannot write; existing evidence is immutable. Cancellation
  wins terminal classification and suppresses an in-flight report.
- API can read only through the authorized reader, worker only through finalization.
  Viewer access works; revoked membership, outside admin and unknown attempts deny.
  Actual responses pass the exported OpenAPI schema, including the engine error type.
- API readiness requires migration 0015; mixed-version workers can continue using
  legacy finalization while deployments roll forward.

Run with a disposable PostgreSQL/pgmq database and the existing test setup:

```sh
cargo test -p labyrinth-worker --test execution --locked
cargo test -p labyrinth-worker --lib --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

No live hosted resources or new retention/destructive cleanup policy are introduced.
Large proofs are explicitly omitted, not persisted elsewhere or silently summarized.

Local checkpoint (2026-11-16): locked workspace/all-features suite **200 passed,
0 failed, 0 ignored**. Default and all-feature Clippy, formatting and diff checks
passed. Worker unit/execution regressions were rerun after the final borrowed-error
classification cleanup and passed. No warning suppressions or skipped tests added.
