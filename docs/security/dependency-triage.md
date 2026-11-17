# Backend dependency advisory triage — 2026-11-17

This is component and affected-operation triage, not a certification that the
application is vulnerability-free. The supported hosted builds are Linux API,
worker (including maintenance binaries), and engine. CLI native targets retain
[their own gate](../testing/cli-packaging.md). The full lockfile remains audited;
optional entries are never erased from the report or added to an ignore list.

## Findings and disposition

- [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387):
  `crypto-bigint 0.5.5` was compiled in the API through the JWT RustCrypto provider.
  The advisory concerns secret-dependent timing, so compilation alone does not
  prove exploitation of this public-key verification service. Upgrade
  `jsonwebtoken 10.4` to 12, selecting `crypto-bigint 0.7.11` (patched >=0.7.6).
- [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071):
  `rsa 0.9.10` was compiled in the API. Production uses trusted JWKS **public-key
  verification**; private signing/key generation exists only in `cfg(test)`
  fixtures. The Marvin advisory concerns private-key operations. Nevertheless,
  use the now-stable patched `rsa 0.10.0`, both through JWT and in test fixtures;
  no private signing keys are persisted or substituted for real signatures.
- [RUSTSEC-2026-0459](https://rustsec.org/advisories/RUSTSEC-2026-0459):
  `libsqlite3-sys 0.30.1` remains an inactive SQLx optional lockfile dependency.
  It was absent from separate compiler inventories of the API, worker and engine.
  The affected operation opens a malicious SQLite database and sibling WAL with
  inconsistent page sizes. Supported services use PostgreSQL and do not open
  SQLite/WAL inputs. Upstream patched range is >=0.39.0. Do not force an ABI/API-
  incompatible transitive override into SQLx 0.8.6; a future SQLite feature or
  component must remediate this before shipping. The compiled-scope gate fails
  if an affected driver becomes selected.
- [RUSTSEC-2024-0449](https://rustsec.org/advisories/RUSTSEC-2024-0449):
  `unsafe-libyaml 0.2.11` was compiled in all three components via
  `serde_yaml_ng 0.10.0`. This is an unmaintained-code warning, not a newly proven
  exploit. Replace that parser with `serde-saphyr 2.1.1`, with only serialization
  and deserialization enabled. Its optional filesystem includes, interpolation,
  comments and robotics extensions are not enabled.

Old crypto versions can remain in Cargo.lock through inactive SQLx MySQL edges.
Do not equate their presence with the new API binary, or call the complete
lockfile clean. Compiler evidence and fresh full findings are stored together.
Other inactive-platform maintenance notices likewise stay visible in full reports.

## Compatibility and parser boundary

The API continues to accept only ES256 and RS256 from the configured Supabase
project issuer; JWT header-selected URLs, HMAC, untrusted role claims and invalid
signatures remain rejected. JWT 12's fallible HMAC-key constructor and new crypto
fixture APIs are handled without weakening production validation. Tests use
fresh ephemeral keys and signed tokens, including unknown critical extensions.
See the [upstream JWT changelog](https://github.com/Keats/jsonwebtoken/blob/v12.0.0/CHANGELOG.md).

Catalog JSON schema, semantic validation and canonical typed-JSON SHA-256 remain
unchanged. A regression pins the saved blueprint catalog hash
`f337bdf91144260729922d1b62f3c88e38d417ce1397b0da8861e3158e34213b`.
The YAML loader keeps its 1 MiB input cap and explicitly bounds depth (64),
documents (1), parser events (200,000), nodes (100,000), anchors/aliases (1,024
of each), recorded anchor events (100,000), recorded anchor bytes (1 MiB), and
scalar bytes (1 MiB). Other parser replay/budget safeguards retain their defaults.
Unknown tags and tagged field names reject; duplicate keys reject by default.
Source snippets are disabled in parse errors. These are parser work budgets,
not a proof of a maximum process RSS or execution time; worker supervision remains
necessary. See [serde-saphyr](https://docs.rs/serde-saphyr/2.1.1/serde_saphyr/).

## Repeatable gate

```sh
python3 packaging/fetch_tools.py --directory .local/backend-audit-tools
python3 packaging/backend_audit.py \
  --tool .local/backend-audit-tools/cargo-audit \
  --output .local/backend-audit
```

Use new/empty output and tool directories. Rust 1.95.0 is the CI toolchain. The
script builds release libraries, **all backend binaries**, and the `migrate`/
`prune_audit` examples shipped as maintenance executables by the Dockerfile. It requires successful
compiler messages for every component, resolves only emitted package IDs against
locked metadata, and runs the checksum-pinned `cargo-audit 0.23.0` against a fresh
advisory database. It rejects audit errors, ignored advisories, target/severity
filters, lockfile changes during collection, compiled vulnerabilities, yanks,
unsoundness and maintenance findings. There is no offline/stale fallback.
Build/metadata/audit operations have finite deadlines.

[Backend dependency CI](../../.github/workflows/backend-dependencies.yml) runs on
pushes and pull requests. Its weekly schedule becomes active when the workflow
reaches the repository default branch (`main`); a `dev` push does not activate that
schedule. Compiler messages, full RustSec
report and scoped summary retain seven days, including on failed gates. Evidence
is host/feature-specific: this gate covers the supported Linux production build,
not every hypothetical target or all-feature combination. Test dependencies and
other platforms require their normal tests and additional review when introduced.
The audit unit tests prove inactive metadata exclusion, active advisory inclusion,
and refusal of missing/unknown component evidence.

## Operational acceptance still outstanding

Live Supabase Auth/Storage, native credential-store round trips, provider quotas,
restore/backup acceptance, end-user distribution terms and signing are independent
open gates. This dependency work does not provision providers or clear those gates.


## Local acceptance evidence

- 233 Rust tests passed, zero failed/ignored, using the complete workspace with all
  features and a real owned PostgreSQL 18.6/pgmq instance. The earlier interrupted
  run is not counted as acceptance; the complete warm-cache rerun exited zero.
- 17 Python packaging/audit tests passed; Ruff lint/format, Rust formatting and
  workspace/all-target/all-feature Clippy with warnings denied passed.
- The production-plus-maintenance compiler inventory contained **261 third-party
  dependency versions**, with **zero blocking and zero maintenance findings**.
- Fresh RustSec database: 1,527 advisories, commit
  `233155b4025b854c68a4b199c0e32d7fd53291a2`, updated 2026-11-17.
  The full report retained inactive SQLite and RSA advisories and the three inactive
  `zbus_names`/`zvariant` maintenance notices. No ignores or filters were used.
- The local Storage provider double now starts with RLS enabled before policy
  bootstrap; scoped read/write/delete/revocation tests pass. This test adjustment
  does not change the separately edited deployment SQL.

Remote workflow results must be checked against the committed source; local Linux
checks do not constitute native CLI or container acceptance for a new lockfile.
