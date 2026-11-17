# CLI packages and GitHub Releases

Labyrinth is proprietary. [LICENSE](../../LICENSE) reserves project rights; it does
not grant end-user rights. Third-party components retain their own licenses. Define
and approve end-user terms before public binary distribution. This workflow prepares
reviewable evaluation packages and drafts; it never publishes a release automatically.

## Targets and prerequisites

Four native jobs build/test the CLI with Rust 1.95.0 and Cargo.lock:

- Linux x86-64 GNU: Ubuntu 22.04 runner, glibc 2.35 baseline, unlocked desktop
  Secret Service needed for real login. The D-Bus C library is vendored.
- Windows x86-64 MSVC: Windows Server 2025 runner; static MSVC runtime build flag.
- Intel Mac: macos-15-intel, minimum deployment target macOS 15.0.
- Apple Silicon Mac: macos-26, aarch64, minimum deployment target macOS 15.0.
  Native runtime tests cover macOS 26 on ARM and macOS 15 on Intel; the deployment
  target is a compatibility build setting, not proof of every older ARM OS version.

Each job runs all CLI tests and strict Clippy, builds an optimized executable,
checks executable architecture and runs the extracted package from a directory with
spaces. Help/version and invalid-config rejection need no Supabase or OS-keyring
session. Mac binaries are ad-hoc signed/verified. Developer ID/notarization and
Windows Authenticode signing are not configured. Native credential-store/live
provider acceptance remains distinct from packaging; do not disable OS security to
run blocked downloads. Source builds remain available.

The standard public-repository runners require no paid runner/provisioning. Artifact
storage still has account limits: native artifacts expire after one day and the
complete release set/security report after seven days. Watch usage and keep spending
limits appropriate to the project's zero-budget policy. No billing settings change.

## What goes into an archive

`laby-vVERSION-TARGET.tar.gz` (Unix) or `.zip` (Windows), with one matching root:

- The CLI executable; README, proprietary LICENSE and BUILD.json.
- FILES.sha256 for every other bundled file, plus THIRD_PARTY_NOTICES.json.
- Dependency license texts, Rust standard-library copyright/licenses, and the exact
  vendored D-Bus source/COPYING for Linux.

The target-filtered Cargo dependency closure excludes dev-only edges and rejects
engine/API/worker/server-auth dependencies. Package inputs are explicit; no `.env`,
credentials, worktree history, catalog data or generated topology is copied. Archive
metadata/file ordering is normalized for identical inputs; this is not a claim of
bit-reproducible executables across toolchains/OS images. BUILD.json records the
source commit, target, Cargo version, canonical-LF lockfile hash and executable
SHA-256. Normalizing only lockfile line endings makes Windows/Unix identities
comparable; package files/executable hashes still cover their exact bytes.

An assembly job requires all four native packages from the same run/source/version.
It rechecks provenance, architecture, complete file inventory and hashes, and emits
SHA256SUMS (four archive checksums) and release-index.json. Release hashes detect
corruption; they do not authenticate a compromised release source or replace signing.

## Preview build

Superseded branch/PR package runs are cancelled; release-tag runs are preserved.
Relevant CLI/Cargo/packaging changes run `.github/workflows/cli-packages.yml` on push
and pull request. `workflow_dispatch` also allows an explicit nonpublishing build.
Download **cli-release-set** from the successful workflow's artifacts. This is a
GitHub Actions wrapper ZIP containing the four actual archives/checksums/index;
extract the wrapper first. The native package jobs and assembly must be green.
The separate **cli-security-report** contains both scoped and workspace audit output.

Validation tools (Ruff and cargo-audit) use exact upstream release archives pinned by
SHA-256 in packaging/tools.lock.json. Tool download/verification fails closed. The
advisory database is fetched during the audit; no stale/offline fallback or advisory
ignore list is configured. The gate audits the union of dependency versions in all
four shipped packages against the workspace lockfile. CLI security/unsound/yanked
findings block; maintenance warnings remain visible for review. Findings outside
that CLI closure stay in the full report and summary for separate triage. A clean
CLI gate does not certify the whole workspace or bundled native C libraries.

## Prepare a draft release

1. Confirm native tests/smokes, dependency report, license notices, current API
   compatibility and release notes. Complete live/provider/native-store and
   operational acceptance before representing a build as production-ready.
2. Update the workspace Cargo version deliberately (currently shared by components),
   lockfile when required, CLI changelog and packaging/cli/RELEASE_NOTES.md. Commit
   and push the reviewed source. No version bump or release tag is created by normal
   packaging CI. Tags must exactly match Cargo's version.
3. Only when deliberately preparing a release, tag that reviewed commit as
   `cli-vVERSION` and push that tag. Keep release tags immutable; use repository tag
   protection/rulesets and a fresh version for changed content. Do not force tags.
4. The tag run rebuilds all four native packages, verifies the complete set and
   runs the dependency audit. Only its draft job gets contents:write; builds and
   pull requests remain read-only, with checkout credentials not persisted.
5. Draft creation rechecks audit/source/lockfile identity and resolves annotated
   tags to reject moved refs. It refuses an existing release, including a draft.
   The new release is always **draft + prerelease**, with explicit curated notes,
   archives, SHA256SUMS, release-index.json and dependency-audit.json attached.
6. Review the actual uploaded draft/assets and remaining distribution/legal/security
   gates before any manual publication. Do not publish automatically from CI.

A failed/uncertain draft upload is never automatically retried or overwritten.
Inspect GitHub first. If an incomplete draft exists, review/remove that draft and
rerun only after determining its state. Never delete/replace a published release to
make the workflow green. Keep previous verified versions for rollback; replacement
packages never touch an installed user's OS credential store.

## Install or update a downloaded package

Choose the exact target. Before extraction, compare the archive with its SHA256SUMS
entry (download both from the reviewed release/run):

```sh
# Linux
sha256sum laby-vVERSION-x86_64-unknown-linux-gnu.tar.gz
# Mac
shasum -a 256 laby-vVERSION-aarch64-apple-darwin.tar.gz
```

On Windows use `Get-FileHash .\laby-vVERSION-x86_64-pc-windows-msvc.zip -Algorithm SHA256`
in PowerShell and compare its result with the matching entry. Extract into a new
user-owned directory, keep all notices, run the included `laby --version`/`--help`,
and optionally add that directory to PATH. No privileged installer, auto-updater,
endpoint or account is bundled. Follow [CLI setup](../../cli/README.md) for public
endpoint configuration, browser login, explicit onboarding and generation.

For an update, use a new directory and switch PATH after verification. Credential
store entries remain outside the package. For uninstall, `laby logout`, remove its
PATH entry and package directory. Published engineering files remain in the backend
until their owner explicitly deletes them under the retention policy.

## Local verification

From a Linux x86-64 development machine (Python 3.12+ and native compiler):

```sh
python3 -m unittest discover -s packaging -p 'test_*.py' -v
python3 packaging/fetch_tools.py --directory .local/cli-validation-tools
.local/cli-validation-tools/ruff check packaging
.local/cli-validation-tools/ruff format --check packaging
cargo +1.95.0 build -p labyrinth-cli --bin laby --release --locked
cargo +1.95.0 metadata --format-version 1 --filter-platform x86_64-unknown-linux-gnu --locked > .local/cli-metadata.json
```

Run the builder with that metadata, executable, a new output directory, exact target
and the source's full commit SHA; then run its smoke subcommand. Use a clean reviewed
checkout for release evidence; dirty local previews are diagnostic only. Windows
builds require RUSTFLAGS `-C target-feature=+crt-static`; Mac release builds require
MACOSX_DEPLOYMENT_TARGET=15.0 plus the workflow's ad-hoc signing step. Execute smoke
on the matching native machine; cross-compilation alone is not native acceptance.

References: [GitHub runner support](https://docs.github.com/en/actions/reference/runners/github-hosted-runners),
[artifact upload](https://github.com/actions/upload-artifact),
[artifact download](https://github.com/actions/download-artifact),
[RustSec cargo-audit](https://github.com/rustsec/rustsec/tree/main/cargo-audit).
