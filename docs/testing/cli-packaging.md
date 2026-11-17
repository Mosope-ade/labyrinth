# CLI packaging acceptance — 2026-11-17

Implementation includes four native build targets, strict Clippy/CLI tests, Python
packaging tests/lint/format, extracted-package smoke, complete-set verification and
a fresh compiler-bound dependency audit. Public release/tag creation is not part of
this implementation run; the draft job stays skipped on branch pushes.

## Verification and regressions

- 15 local packaging tests pass: deterministic layout, native executable headers,
  path/type/duplicate extraction rejection, tag/version/provenance, full-set
  checksums, draft refusal/moved refs, audit scope/filter controls, compiler evidence
  and LF/CRLF lock identity.
- The existing 20 CLI library/executable tests run natively on each target; no tests
  were ignored or compiler/lint warnings suppressed.
- Actual Windows CI exposed Unix-only filesystem-mode assertions and an unnecessary
  mutable directory-builder binding. Archive modes are checked on every platform;
  filesystem executable bits are checked only where the OS represents them.
- Actual Windows artifact inspection proved its original Cargo.lock fingerprint was
  exactly the CRLF form of the Unix file. Canonical-LF identity now gates all targets;
  package file and executable hashes remain raw-byte checksums.
- GitHub failed to provision one free macOS 15 ARM job after repeated attempts.
  The current free macOS 26 ARM image passed; Intel uses macOS 15. Minimum deployment
  target is 15.0, distinct from the OS actually used for native runtime tests.
- Compiler messages, not declaration metadata alone, define the shipped build scope.
  The successful local Linux release recorded 154 package versions, with zero
  security/maintenance findings in that compiled set at the audited database snapshot.
  Unbuilt optional crates accounted for three earlier maintenance warnings.

Check [CLI packages CI](https://github.com/Mosope-ade/labyrinth/actions/workflows/cli-packages.yml)
for the exact source commit/run. Successful runs expose cli-release-set (four archives,
SHA256SUMS, release-index.json) and cli-security-report (scoped and full reports).
Native artifacts retain one day; aggregate/security artifacts retain seven days.

## Security scope and remaining work

The fresh full-lockfile scan retained advisories outside the CLI build scope:

- RUSTSEC-2026-0387: crypto-bigint 0.5.5 timing variability.
- RUSTSEC-2026-0459: libsqlite3-sys 0.30.1 / bundled SQLite.
- RUSTSEC-2023-0071: rsa 0.9.10 / Marvin timing attack.

These are recorded findings, not advisory waivers or proof that deployed backend
operations are exploitable. Component-specific compiler/feature and affected-
operation review is required before upgrade/reachability decisions. Optional and
dev-only packages can appear in the workspace lockfile without being shipped.
The CLI gate retains the full evidence and does not claim workspace-wide security.

Local/provider doubles and native package smoke do not prove real SMTP/PKCE,
native credential-store round trips, provider quotas, backups/restore or signing.
Supabase setup is deferred to the user. Public distribution still requires end-user
terms and separate acceptance; the proprietary notice is not a customer use grant.
See [release ADR](../decisions/0011-native-cli-release-artifacts.md),
[runbook](../runbooks/cli-releases.md) and [CLI usage](../../cli/README.md).

## Recorded code acceptance

Source commit `40044088b1dd2658725cc6a081c2cc6185f49967`: all four native targets,
full release-set verification and dependency audit passed in
[CLI packages run 44087052010](https://github.com/Mosope-ade/labyrinth/actions/runs/44087052010).
Engine contracts, PostgreSQL admission/integration and backend container workflows
also passed for that code commit. The branch-triggered draft job was correctly
skipped; no release tag, draft or public GitHub Release was created.

The final four-target audit covered 165 third-party dependency versions from actual
compiler output, with zero blocking findings and zero maintenance warnings in
that CLI scope. The complete lockfile report remains available separately.


## Backend advisory follow-up

The original outside-CLI findings have now been assessed against separate backend
compiler inventories. See [dependency triage](../security/dependency-triage.md) for
patched crypto dependencies, the maintained catalog parser, inactive SQLite/MySQL
lockfile edges and the new independent backend gate. Historical CLI acceptance above
remains scoped to its recorded source commit; a new lockfile requires fresh native CI.
