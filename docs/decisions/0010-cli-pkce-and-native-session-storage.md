# ADR 0010: Native CLI PKCE and session persistence

## Status
Accepted for the source-built client; hosted/native-platform acceptance pending.

## Context
The CLI must authenticate human users against the same Supabase project as the GUI
and call the existing resource API. The user selected magic links with PKCE. The
provider is not yet provisioned. Tokens must not be passed as arguments or saved in
ordinary configuration files, and rotating refresh tokens must survive invocations.

## Decision
Use S256 PKCE with OS randomness, a pre-bound IPv4 loopback callback, independent
random state and a five-minute deadline. Correlation and bounded code parsing happen
before consuming the one-use callback. The provider verifies the code/verifier;
no proxy/password or technical-account human login is added. The browser completion
page reflects no input and removes the displayed callback URL. A received callback
is not a successful login until exchange and persistence complete.

Use keyring 3.6.3's explicit native adapters (Linux Secret Service, macOS Keychain,
Windows Credential Manager), with no default/mock or plaintext fallback. The locked
3.x API supports the needed operations; no system keyring is replaced or configured
by the application. Vendored libdbus avoids a Linux development-header prerequisite;
security updates require rebuilding the executable. Keep credentials scoped to both
API and Auth origins and serialize commands using OS advisory locks. Persist only
the refresh token and provider UUID, saving rotation before any API call. Enforce
subject continuity, bounded replies, fixed redacted errors and redirect refusal.

Logout attempts provider local-session revocation and erases local credentials even
when the provider is unavailable. It reports incomplete revocation. Existing access
JWTs may survive until expiry; server-side suspension/membership checks remain vital.
Signing in may register an Auth user but never creates application workspaces.

## Consequences
An unlocked desktop credential store and same-computer browser are required. No
headless fallback, wildcard hosted origin, automatic workspace creation, runtime
secret/service key, or API CORS change is introduced. Local HTTP and adapter tests
are distinct from live provider, native-store and cross-platform packaging evidence.
See [CLI contract](../contracts/cli-v1.md) and [runbook](../../cli/README.md).
