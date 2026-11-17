# Hosted CLI authentication V1

`laby` is a separate Rust HTTPS client with no engine/API implementation dependency.
Public configuration: `LABY_API_URL`, `LABY_SUPABASE_URL` (project root), and
`LABY_SUPABASE_PUBLISHABLE_KEY`. No service/secret keys, user passwords or bearer
arguments. API HTTP is permitted only at literal 127.0.0.1 for local Docker;
Auth always requires HTTPS. Redirects and credential forwarding are disabled.

Commands in the first slice: `login`, `logout`, `whoami`, `onboard`, `tenants`,
`projects TENANT`, `project-create TENANT NAME --idempotency-key KEY`.
Read commands print JSON; mutations are explicit. Signing in never onboards.
No admin role or technical account is used by this client.

Login prompts for email on stdin and sends Supabase `/auth/v1/otp` with S256 PKCE,
`create_user=true`, and a correlated redirect. This may create a provider Auth user,
not a Labyrinth workspace. Bind IPv4 loopback before sending, default port 54327,
overridable with `--callback-port` (allowlist the exact chosen port). The browser
email link must be opened on the same computer while the command is running.
Callback: `http://127.0.0.1:54327/callback?state=RANDOM`. Supabase must allow
`http://127.0.0.1:54327/callback?state=*` (a query-only wildcard; host,
port and path are fixed). Supabase matches the query string too; an exact entry
without the state query can fall back to Site URL on some deployed versions. Callback validation
requires exact Host, path, unique code/state, matching random state and a bounded
query. Foreign requests cannot consume the pending verifier. Login expires after
five minutes, can be interrupted, and exchanges the code only once. Callback HTML
contains no reflected input and clears the displayed URL; no CORS or API route is
added. Verifier/state/access token remain in process memory.

Persist only the rotated refresh token and provider subject in the OS credential
store (Linux Secret Service, macOS Keychain, Windows Credential Manager), keyed by
both API and Auth origins. Fail closed if unavailable/locked; no plaintext fallback.
An OS advisory file lock serializes local login/refresh/logout across processes;
OS releases it after a crash. Access tokens are refreshed for each invocation and
never printed. Save rotation before calling the resource API. Refresh must preserve
the same subject. Provider replies and callback URLs are never logged.
Logout requests Supabase `scope=local` and erases the local credential even if the
provider is unavailable, reporting incomplete remote revocation. Already-issued
JWTs may survive until expiry; application authorization still applies.

Request/body/time bounds and safe fixed errors apply throughout. Mutations are
never automatically retried. Preserve the idempotency key when an outcome is
unknown. Provider CAPTCHA, remote/SSH browser handoff, scripted credentials, GUI
and release packaging are separate slices.

Native tests use loopback provider/API doubles and an injected in-memory credential
store; they do not prove actual keychain integration, SMTP, redirect allowlists or
live Supabase code replay/refresh/logout. Those remain explicit hosted acceptance
gates once the user provisions Supabase.

References: [Auth REST schema](https://github.com/supabase/auth/blob/master/openapi.yaml),
[PKCE](https://supabase.com/docs/guides/auth/sessions/pkce-flow),
[redirect matching](https://supabase.com/docs/guides/auth/redirect-urls),
[keyring 3.6.3](https://docs.rs/keyring/3.6.3/).

## Structured workflow extension — 2026-11-17

Implemented `catalogs`, `validate TENANT INPUT`, `generate TENANT PROJECT INPUT
--idempotency-key KEY`, `jobs`, `status`, `cancel`, `attempts`, `revisions`, `inspect`
and `download TENANT PROJECT REVISION OUTPUT`. List commands accept limit 1–100 and
an explicit after cursor; status is a single persisted read, with manual polling.
Structured input uses existing HTTP envelopes and is transmitted byte-for-byte,
with a 2 MiB limit. Admission and generation remain distinct. No AI or engine is
embedded. API error code/details are bounded JSON, escaped for terminal output;
provider Auth replies remain redacted. Mutations are not automatically retried.

Downloads authenticate once per invocation, reauthorize through each resource API
request, and verify the exact fixed inventory, revision/manifest hash, file hashes
and sizes (32 MiB aggregate). No disk output until all checks pass. Save only to a
new directory, no overwrite; Unix modes 0700/0600, inherited Windows ACLs. Write the
manifest last; a crash may leave a partial directory requiring explicit inspection.
This checks transport integrity against authenticated server metadata; it does not
independently repeat engine verification. Full AI-free workflow commands supersede
the earlier deferred-command statement. Optional AI, release packaging and real
provider/native-store acceptance remain open.
