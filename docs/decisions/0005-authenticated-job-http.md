# ADR-0005: Authenticated job HTTP boundary

## Status
Accepted for user-token admission and status polling. Login/provisioning, CLI automation
credentials, worker execution and artifact downloads remain separate slices.

## Date
2026-11-15

## Context

Both hosted clients need the same asynchronous job API. Client-supplied actors, tenant
claims and role metadata cannot authorize access. The API must reject untrusted tokens
before querying private data and must preserve the database transaction's guarantees.

## Decision

Use Axum routes under `/v1/tenants/{tenant}/projects/{project}/jobs`. Authenticate one
Authorization bearer access token with the configured Supabase project's asymmetric
JWKS. Accept ES256/P-256 or RS256 public keys only, pinned by `kid` and algorithm.
Never use legacy shared-secret, anon or service-role tokens as user identity.
Require exact issuer, authenticated audience, valid expiry/nbf, issued-at, UUID subject,
non-nil session ID, authenticated role and `is_anonymous=false`. Custom admin metadata
has no authority. Expiry has no grace; issued-at allows 30 seconds of clock skew.

Derive the JWKS endpoint only from trusted startup configuration. Use HTTPS, certificate
verification, no redirects, bounded fetch time/size/key count, a five-minute local
cache and a thirty-second refresh cooldown. A single refresh lock coalesces misses;
fresh known keys remain usable while a refresh is pending. Expired keys cannot be
used if refresh fails. Startup requires a usable key set. Unknown kids can be delayed
by the cooldown during rotation; operators must retain key overlap. Upstream JWKS
caching may add delay beyond the local cache and is outside our revocation guarantee.

Signature verification is not online session introspection: Supabase logout stops
refresh, but an issued access token can remain valid until expiry. Every business
request separately checks current application user/tenant suspension and membership
in Postgres. Removing application access therefore does not wait for JWT expiry.
This does not implement browser sessions, refresh storage, signup or CLI login.

Only the trusted verifier constructs `AuthenticatedActor`. Request bodies cannot
supply an actor. The API loads catalog YAML through an authorized read function,
revalidates its hash and prepares immutable input; the admission function rechecks
all authorization/head/quota/catalog conditions atomically before enqueueing.
Status reads use a separately authorized function. Viewer members may read status;
only owner/editor members may submit. No direct runtime table grants are added.

Bound request bodies, active requests and duration. Blocking preparation retains its
capacity permit even if the awaiting HTTP request is cancelled. A timed-out submission
may have committed: retry the same key. Never equate an HTTP timeout with job rollback.
Startup requires explicit pool/time limits, verifies hosted database TLS, rejects
privileged database logins and selects only the restricted API role.

## Consequences

The API can be tested with real asymmetric signatures, a local JWKS HTTP test issuer
and real PostgreSQL/pgmq without hosted credentials. Plaintext JWKS overrides exist
only in test-compiled code, not the production binary. There is no unsigned test-token
or development-auth bypass in the server.

Responses use stable JSON errors, no-store and request IDs. Logs contain request ID,
status and elapsed time, not tokens, input payloads, object keys or private claims.
Public `/health/live` exposes process liveness only. TLS terminates at the hosting
ingress; the database and remote JWKS connections verify TLS independently. CORS is
not opened in this slice; exact GUI origins must be configured before browser delivery.

## References

- [Supabase signing keys](https://supabase.com/docs/guides/auth/signing-keys)
- [Supabase JWT claims](https://supabase.com/docs/guides/auth/jwt-fields)
- [Supabase sessions](https://supabase.com/docs/guides/auth/sessions)
- [Axum middleware](https://docs.rs/axum/0.8.9/axum/middleware/index.html)
- [jsonwebtoken](https://docs.rs/jsonwebtoken/10.4.0/jsonwebtoken/)
