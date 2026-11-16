# Attempt diagnostic acceptance

Use the locked workspace/all-features PostgreSQL/pgmq suite and the usual formatting
and Clippy-with-warnings-denied gates. Tests never require hosted credentials.

- Signed-request/PostgreSQL API test: empty/running/finished attempt history,
  resource-limit=inconclusive, viewer access, outsider/unknown-job denial, and absence
  of worker identity, lease/fence, private prefixes, inputs and object keys.
- Worker database test: transient reporting and finalization persist atomically,
  duplicate/stale reporting cannot overwrite the recorded category, scheduled retry
  time disappears after retry exhaustion.
- Real supervised worker/storage fault integration: failed storage records the
  allowlisted storage_unavailable diagnostic while preserving retry/publication
  behavior and historical published bytes.
- Real subprocess regression: a cooperative selection-search budget exhaustion maps
  to resource-limit classification both directly and across the child exit protocol.
  Existing hard CPU/memory/deadline tests still pass.

Synthetic API history rows exercise the public response only; they do not claim
real engine generation. Worker tests cover that separately. No full counterexample
payload or hosted diagnostic-retention policy is introduced by this slice.

Local acceptance (2026-11-16): locked workspace/all-features suite passed **184 tests,
0 failed, 0 ignored**. Formatting and workspace Clippy with warnings denied passed.
