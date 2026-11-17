# Docker guides and scoped Storage acceptance

Local locked all-feature workspace suite: **210 passed, 0 failed, 0 ignored** on
2026-11-17. No real provider account was provisioned or contacted for authentication.

Coverage:

- Distinct publishable apikey and bearer headers in reader/worker HTTP transports.
- Login, refresh rotation, invalid-refresh password fallback and concurrent coalescing.
- Provider outage backoff, redacted errors, response-size bounds and redirect rejection.
- Suspend/wall-clock expiry, Storage 401 cache invalidation without replaying writes,
  origin confinement and unsafe configuration/header rejection.
- Exact reviewed provider SQL exercised twice in an isolated PostgreSQL database with
  minimal auth/storage fixtures. Technical reader/worker/human JWT subjects are modeled
  through transaction-local auth.uid claims under the ordinary authenticated role.
- Bucket scoping, live-attempt writes, immutable/no-overwrite behavior, publication-only
  reads, active-published deletion denial, 24-hour orphan eligibility, seven-day explicit
  purge eligibility, immediate principal disable and metadata-only operator audits.
- Bootstrap refusal of unreviewed permissive policies. These native fixtures do not
  replace testing actual Supabase JWT-role mapping and Storage HTTP behavior.

Reproduce with disposable PostgreSQL/pgmq setup:

```sh
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

The new workspace crate is included in the Docker build-context whitelist and build
stage. Shell blocks in both setup guides pass bash syntax checks. The root unused
.env.example is removed; component templates remain and contain placeholders only.
The workspace has a Docker client but no running daemon, so a native fixture check
is distinct from the actual image build/smoke workflow in remote CI.

Live acceptance after the operator completes the root guide: real technical login/
refresh, private-bucket startup check, scope-negative requests with genuine provider
JWTs, structured generation/publication/download, revocation and cleanup. Keep actual
credentials and raw login responses out of logs, Git and chat.

The native offline fixture was also generated under prlimit with an empty environment,
verified by the same package verifier, and unpacked into 13 JSON artifacts using the
guide's procedure. Provider blueprint schema validation passed without provisioning.
Both default/all-feature Clippy gates, formatting and diff checks passed.
