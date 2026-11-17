# ADR 0011 — Native CLI artifacts and draft-only releases

Status: accepted, 2026-11-17. Public distribution remains gated separately.

## Context

The user selected GitHub Releases for CLI distribution and proprietary project
licensing. The project has no funding for paid runners or signing infrastructure.
The CLI is a hosted API client; packages must not embed engine/server components,
credentials or generated topology data. Supabase remains unprovisioned by the user.

## Decision

Build native Linux x86-64 GNU, Windows x86-64 MSVC and Intel/Apple Silicon Mac
archives with pinned Rust/Cargo.lock. Native tests, strict lint and extracted-binary
smoke checks gate every target. Windows uses the static MSVC runtime. Mac executables
are ad-hoc signed, with a macOS 15 deployment target; current native test images are
macOS 15 Intel and macOS 26 ARM. Deployment settings do not prove every older OS.

Use a small standard-library packaging/REST pipeline for explicit artifact inventory,
source/version/hash checks, vendored native-source notices, and refusal to overwrite
existing drafts/releases. A release framework such as cargo-dist remains an option
if the target set or distribution features expand. This implementation carries the
maintenance cost of archive/REST adapters; tests and native CI are required.

Intersect the target-filtered, non-dev declaration graph with successful Cargo
compiler-artifact messages. Metadata alone includes inactive optional dependencies:
initial traces listed 225 dependency versions and maintenance findings for unbuilt
crates. Real Linux release compilation contained 154 packages. Both compiled normal
and build dependencies are covered, with Rust standard-library notices separate.
Compiler evidence must contain exactly one successful completed build and the CLI;
undeclared/unexpected compiled inputs reject. No advisory ignore, severity threshold
or target-filter shortcut is allowed in audit reports.

Canonicalize only Cargo.lock line endings for its provenance fingerprint: actual
Windows artifacts initially hashed CRLF differently from Unix LF. All four packages
must match the reviewed lock identity. Package/executable inventories use exact-byte
SHA-256. Archives have normalized ordering/metadata; executable bit reproducibility
across OS/toolchain images is not promised.

Tag runs must match Cargo's version, retain immutable source identity, contain all
four native packages, pass the compiler-bound audit and resolve an unmoved tag.
Build/PR jobs are read-only. Only the tag-triggered draft job gets contents:write;
it creates a new draft prerelease and never publishes/overwrites it. Superseded
branch builds may cancel, while release-tag runs are preserved. Unknown upload
outcomes require inspection rather than automatic replay or destructive repair.

## Consequences

The project LICENSE is a proprietary rights notice, not an end-user agreement.
Third-party licenses remain separate. End-user terms, Developer ID/notarization,
Authenticode, real credential-store/Supabase and operational acceptance remain open.
Checksums detect corruption, not a compromised distribution source. CI artifacts
expire; published GitHub Releases are a separate deliberate operator action.
Full workspace audit findings outside the CLI stay visible for component-specific
reachability/upgrade triage. A CLI pass does not certify the entire project or its
native C dependencies. See [release runbook](../runbooks/cli-releases.md).
