# Labyrinth hosted CLI (`laby`)

A separate Rust client of the hosted API. It does not embed the engine and does not
need an AI key for structured requests. Authentication is Supabase email magic-link
PKCE. Human sessions and backend technical Storage identities are separate.

## Install from source

Use Rust 1.95.0 (the repository toolchain) and a native C build toolchain:

```sh
cargo install --path cli --locked
laby --help
```

Linux builds vendor libdbus for the credential adapter; a running, unlocked desktop
Secret Service (e.g. GNOME Keyring/KWallet with Secret Service enabled) is required
at runtime. macOS uses Keychain; Windows uses Credential Manager. No plaintext
credential fallback exists. Headless/SSH browser authentication is not supported
by this slice. GitHub binary packaging is a later release task.

## Configure and sign in

After following [Supabase setup](../SUPABASE_SETUP.md), set public client configuration
in your terminal (these are not backend database/Storage credentials):

```sh
export LABY_API_URL=http://127.0.0.1:3000
export LABY_SUPABASE_URL=https://YOUR_PROJECT.supabase.co
export LABY_SUPABASE_PUBLISHABLE_KEY=sb_publishable_YOUR_PUBLIC_KEY
laby login
```

Use HTTPS for deployed APIs. Only literal IPv4 loopback HTTP is allowed for local
Docker. Auth always requires HTTPS. Supply your email at the prompt, then open the
received link in a browser **on this computer**. Keep the terminal running; the wait
expires after five minutes. Ctrl+C cancels it. A successful browser callback is not
proof of login until the terminal confirms the exchange and secure session save.

Allowlist `http://127.0.0.1:54327/callback?state=*` in Supabase Auth; this wildcard is
limited to the query, with a fixed loopback host/port/path. Keep the magic-link email
template's provider `ConfirmationURL` flow; do not replace it with implicit token
URLs or a custom endpoint that bypasses the PKCE code redirect. Provider versions
may accept loopback redirects independently of the list; verify actual settings.
If port 54327 is occupied, use `laby login --callback-port 54328` and allowlist the
corresponding URL. Do not expose the listener via a public interface/tunnel.
Login may register a Supabase Auth user when signup is enabled. It creates no app
workspace, project, admin role or infrastructure resource.

```sh
laby whoami
laby onboard
laby tenants
laby projects TENANT_UUID
laby project-create TENANT_UUID 'First design' --idempotency-key first-design-001
laby logout
```

`onboard` explicitly provisions/retrieves your personal workspace. Save its tenant_id.
List commands return the first server page in this slice. Reuse the project creation
key after an uncertain response; use a different key for another creation intent.
Successful resource commands print JSON. Errors use nonzero exit status; auth errors
never include provider responses, emails, callback URLs or tokens.

Refresh tokens are stored only in the OS credential store, scoped to both API/Auth
origins. Each resource invocation refreshes and saves token rotation before calling
the API. A nonblocking advisory profile lock prevents simultaneous local commands
from racing rotation; retry after the current command finishes. Access tokens,
verifier and state are never persisted. `logout` requests local-session revocation
and erases the credential even if remote revocation fails; an incomplete result is
reported. An issued access JWT can remain usable until expiry.

## Troubleshooting and verification

- Missing configuration: set all three LABY variables or use the corresponding global
  CLI flags from `--help`. Never pass bearer/refresh tokens, passwords or service keys.
- Credential store unavailable: unlock/configure your desktop keyring. Do not work
  around this with a token file. Different OS login sessions may have different stores.
- Login timed out: verify email delivery, callback allowlist/template and same-machine
  browser use; rerun login. Links are bound to the initiating verifier and one use.
- Authentication rejected after refresh: the saved invalid credential is removed;
  sign in again. Provider outages preserve the credential for a later retry.
- HTTP 403: check current tenant membership and suspension; login does not grant roles.
- HTTP 409 after mutation: inspect the original key/body and project state; do not
  retry a changed request under the same key. No mutation is automatically replayed.

```sh
cargo test -p labyrinth-cli --locked
cargo clippy -p labyrinth-cli --all-targets --locked -- -D warnings
```

Local tests use actual loopback HTTP servers and an in-memory credential adapter.
They cover PKCE/callback abuse, code replay, rotation/revocation, failure redaction,
locks and executable parsing. Live SMTP, deployed Supabase and native desktop
credential stores need separate acceptance once the provider is prepared. Full
structured generation/download commands follow this authentication slice.
See [CLI contract](../docs/contracts/cli-v1.md).
