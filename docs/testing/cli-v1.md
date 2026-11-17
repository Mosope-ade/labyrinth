# CLI V1 acceptance — 2026-11-17

Local verification: **230 passed, zero failed/ignored** across the workspace with
all features and real PostgreSQL/pgmq. The CLI contributes 18 library HTTP/logic
tests and two executable tests. Full and default workspace Clippy use `-D warnings`;
format/diff and runbook shell syntax checks also pass. Existing device-selection
and worker deadlines were not relaxed.

CLI coverage includes:

- S256 reference vector, independent OS randomness and profile origin restrictions.
- Actual loopback browser callback → one-use code/verifier exchange → saved refresh
  credential; foreign/duplicate/oversized requests, replay, timeout and bind conflicts.
- Refresh rotation before resource requests, same-subject checks, rejected refresh,
  provider outage, logout/revocation and credential-save failure before API use.
- No mutation replay for redirects/401/403/409/429/500; no provider-body disclosure.
- Raw JSON envelope preservation (including duplicate keys), stable idempotency
  headers, safe structured API errors, input bounds and explicit pagination.
- Full synthetic 13-file download, manifest/inventory/size/hash checks, corrupt,
  duplicate, unknown/traversal and oversized metadata rejection before disk output;
  output overwrite refusal and Unix private modes.
- Advisory profile locks and executable help/version/argument failures without
  requiring a live desktop keyring or provider credentials.

Commands:

```sh
cargo test -p labyrinth-cli --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
# With an isolated PostgreSQL/pgmq test database:
cargo test --workspace --all-features --locked
```

The HTTP servers simulate provider/API contracts; they do not send emails or run a
real Supabase service. Credential-adapter tests use an in-memory store. Linux native
Secret Service, macOS Keychain, Windows Credential Manager and packaging still need
platform acceptance. Live email/template/allowlist, verifier/code replay rejection,
refresh reuse/rotation/logout and the complete persisted backend workflow remain
pending the user's Supabase setup. Local tests do not prove cloud availability,
backups/restore, quota sizing or production readiness.

Use [CLI README](../../cli/README.md), [contract](../contracts/cli-v1.md),
[ADR 0010](../decisions/0010-cli-pkce-and-native-session-storage.md) and the root
[Supabase guide](../../SUPABASE_SETUP.md). No new migration is required by the CLI;
the current hosted backend still requires migrations through 0016 plus its reviewed
provider-specific Storage bootstrap.
