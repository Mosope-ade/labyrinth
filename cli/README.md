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
List commands return one bounded server page; use --after to continue. Reuse the project creation
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
credential stores need separate acceptance once the provider is prepared. Structured generation/download usage follows below.
See [CLI contract](../docs/contracts/cli-v1.md).

## Structured generation and downloads

The engineering commands are now available. They call the existing hosted backend;
validation does not generate a topology, and job admission is not verification.
A worker must be running and an operator must have admitted a catalog first.

```sh
laby catalogs TENANT_UUID --limit 25
laby projects TENANT_UUID --limit 25
laby jobs TENANT_UUID PROJECT_UUID --limit 25
laby revisions TENANT_UUID PROJECT_UUID --limit 25
```

For another page, supply `--after NEXT_AFTER` from the prior response. Listings are
not snapshots; UUID/hash ordering is not creation-time ordering. No unbounded
client-side page walk is performed.

Prepare two JSON envelopes from a complete structured intent (the committed fixture
is synthetic example data, not production sizing). Save the admitted catalog hash
from `catalogs`, then create files locally:

```sh
umask 077
mkdir -p .local/cli
python3 - <<'PY'
import json
from pathlib import Path
intent = json.loads(Path('tests/fixtures/intents/enterprise.json').read_text())
Path('.local/cli/validate.json').write_text(json.dumps({'intent': intent}))
Path('.local/cli/generate.json').write_text(json.dumps({
    'intent': intent,
    'catalog_sha256': 'REPLACE_WITH_ADMITTED_64_CHARACTER_HASH',
    'expected_head_revision_id': None,
}))
PY
laby validate TENANT_UUID .local/cli/validate.json
laby generate TENANT_UUID PROJECT_UUID .local/cli/generate.json --idempotency-key design-001
laby status TENANT_UUID PROJECT_UUID JOB_UUID
laby attempts TENANT_UUID PROJECT_UUID JOB_UUID
```

Edit the placeholder hash before submitting. The null head is only for an empty
project; for an existing project supply its exact current head UUID. Incremental
requests must also carry the matching core revision pins. The CLI preserves the raw
JSON bytes, including duplicate fields, so backend validation can reject ambiguity.
Inputs are capped at 2 MiB; backend intent/options constraints apply independently.
There is no automatic mutation retry: reuse the original key **and unchanged body**
when an admission outcome is uncertain. Structured API clarification/validation
code and bounded details are printed safely on stderr with a nonzero exit status.

Invoke `status` again to poll. Terminal failed/cancelled states are job results,
returned as JSON; they do not by themselves prove engineering infeasibility.
`attempts` provides the fixed diagnostic categories and assurance outcomes.
`laby cancel TENANT_UUID PROJECT_UUID JOB_UUID` explicitly requests cancellation.
On success, save the returned revision UUID:

```sh
laby inspect TENANT_UUID PROJECT_UUID REVISION_UUID
laby download TENANT_UUID PROJECT_UUID REVISION_UUID .local/first-package
```

Download verifies authoritative revision identity, the fixed 13-file inventory,
32 MiB aggregate bound, exact sizes/SHA-256, manifest hash and its per-file entries.
It fetches through authorized API routes with one in-memory access token and never
uses private Storage keys. JSON is checked before writing. Existing paths, unknown
filenames, duplicates, traversal and mismatches fail closed. On Unix the newly
created directory/files are mode 0700/0600; Windows uses inherited filesystem ACLs,
so choose a private location. The manifest is written last. A process/machine crash
can leave an incomplete directory; choose a new directory for retry and inspect the
old one before removing it. No existing output is overwritten or recursively erased.
Transport/hash verification does not re-run the engineering verifier or create an
independent proof of the package's claims.
