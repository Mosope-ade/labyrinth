# Retention and project lifecycle acceptance

Local checkpoint: **202 passed, 0 failed, 0 ignored**, locked workspace/all-features
suite on disposable PostgreSQL/pgmq (2026-11-16). This is local integration evidence;
provider-managed backup expiry and live hosted restore remain separate acceptance.

## Exercised behavior

- Confirmed owner-only delete/restore, editor denial, normal-content access and new
  admission blocked while deleted, deleted-project discovery and stable delete replay.
- A held job-row lock deliberately keeps an old worker attempt live across deletion
  and restoration. Its lease is still unexpired, but heartbeat/context/publication
  fail under the retired epoch. Bounded cancellation follows; newly admitted jobs
  capture the restored epoch. No old publication can become visible.
- Real hosted generation creates published packages in two projects. Cleanup leaves
  bytes intact during recovery, supports restoration, refuses recovery after expiry,
  retries partially failed deletion and leaves the other project's bytes unchanged.
  A subsequent repeat sweep removes a late stale upload.
- Deletion during the first artifact upload stops subsequent writes and publication.
- Young failed inputs are preserved; 30-day-expired input/provenance/options, raw keys
  and evidence are erased. Fingerprints preserve idempotent replay. Successful active
  project inputs stay intact. Purged creation names/keys are erased, while an old
  creation key cannot accidentally create another project.
- Upgrading from migration 0015 with historical terminal jobs gives them a fresh
  conservative completion window; immutable guards remain active and running migrations
  again is checksum-compatible. Old private worker bypass functions are not executable
  by API/worker roles.
- A simulated old DB state is reconciled using an independently exported lifecycle
  ledger through the isolated recovery role. Replays/older epochs are ignored,
  project access remains blocked, cleanup receipts reset and unrelated content survives.
  API, worker and routine audit-maintenance roles cannot invoke recovery functions.
- HTTP response payloads pass the OpenAPI contract, which now includes confirmed
  deletion, restore and deleted-project pagination.

The cancellation regression first reproduced an empty PID-file read (`PID=""`), which
made the test inspect `/proc/` rather than a child. The fixture now deliberately
creates an empty-file interval, and the test waits for a valid positive PID before
aborting. The existing startup/reaping deadlines and process limits were retained.
No test was skipped and no warning suppression was added. A timing-sensitive core
case exceeded its existing deadline during concurrent compilation; the final suite
was rerun without competing build checks and passed without changing the deadline.

## Reproduce

Use the existing disposable PostgreSQL/pgmq setup and LABYRINTH_TEST_DATABASE_URL;
never point this suite at a retained environment. Upgrade/hosted tests create their
own UUID-named databases. Administrative fixture time travel stays inside those
isolated databases; it is not an application retention bypass.

```sh
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Run CPU-deadline tests separately from build/lint workloads. Focused entry points are
`api/tests/retention_upgrade.rs`, the project-deletion HTTP test in
`api/src/http/database_tests.rs`, and `worker/tests/hosted.rs`. Storage adapter tests
continue to cover exact-key inventories, credentials, unsafe-key rejection and HTTP
failure retries. See [policy](../contracts/retention-v1.md) and
[recovery runbook](../runbooks/retention-and-recovery.md).

Final review added explicit quota-release and active-project ledger-reconciliation
scenarios. The upgrade and real hosted worker tests passed again with those checks.
Recovery functions are denied to routine audit maintenance as well as API/worker
roles; the separate offline `labyrinth_recovery` group receives only the two ledger
ports. The restored active project's published bytes remain intact.
