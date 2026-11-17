# Labyrinth CLI (`laby`)

A hosted client: no embedded engine, database, server secrets or offline generation.
An AI key is not needed for structured engineering requests. A configured Labyrinth
backend and Supabase project are needed for login/generation. Help/version work offline.

Check SHA256SUMS against the downloaded archive before extraction. Download the
checksum file and package from the same reviewed GitHub release; hashes detect
corruption, not a compromised download source. BUILD.json records source commit,
target, Cargo.lock hash and executable hash; FILES.sha256 records bundled files.
Keep the entire extracted directory (including license/notices), and add its location
to PATH if desired. Nothing is installed into privileged directories automatically.

Targets: Linux x86-64 GNU (built/tested on Ubuntu 22.04, glibc 2.35 baseline),
Windows x86-64 MSVC (tested on Server 2025), macOS 15+ Intel or Apple Silicon.
Linux needs an unlocked desktop Secret Service; Windows uses Credential Manager;
Mac uses Keychain. No plaintext fallback. Headless/SSH browser handoff is unsupported.
The package has no configured endpoint, stored login or backend credentials.

These initial packages are evaluation builds. Windows binaries have no Authenticode
signature. Mac binaries have an ad-hoc signature, not Developer ID signing/notarization.
Do not disable platform security to run a blocked download; build from reviewed
source or wait for signed distribution. Live provider/native-store acceptance is
separate from package smoke tests; packaging alone is not production readiness.

Set the public LABY_API_URL, LABY_SUPABASE_URL and LABY_SUPABASE_PUBLISHABLE_KEY for
YOUR environment, then run `laby login`. Never use backend technical accounts,
service-role keys, DB passwords or bearer tokens as client configuration. Login sends
a magic link: open it on the same computer while the terminal waits. The callback
uses IPv4 loopback port 54327 by default; allowlist its correlation-query URL as
documented in the source guide. Signing in creates no application workspace.

Start with `laby --help`, `laby whoami`, then explicitly `laby onboard`. Generation,
job status/cancellation, revision inspection and verified downloads use the hosted
API. `laby logout` erases the local credential and attempts local-session revocation.
Updates replace the executable/package directory; OS-stored sessions live separately.
Keep the previous verified package for rollback; do not roll back across incompatible
API contracts. Uninstall by logging out, removing the package and its PATH entry.

Full source/install/usage guide:
https://github.com/Mosope-ade/labyrinth/blob/main/cli/README.md
Operator release guide:
https://github.com/Mosope-ade/labyrinth/blob/main/docs/runbooks/cli-releases.md

Third-party dependency metadata/license texts are included under licenses/ and
THIRD_PARTY_NOTICES.json. Linux includes the exact vendored D-Bus C source/COPYING
under native-sources/. Labyrinth itself is proprietary; see LICENSE. Third-party notices do not license
Labyrinth's own code. The proprietary notice is not an end-user agreement; public
distribution requires separate approved end-user terms.
