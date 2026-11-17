# Server-only Storage identities

Shared API/worker session transport; no engine or CLI dependency. Human authentication
remains passwordless PKCE. Runtime callers supply modern publishable gateway key plus
one dedicated technical Auth email/password. Origin must be a clean HTTPS root;
credentials are sent only to its fixed Auth token endpoint and Storage origin.
Static bearer injection exists only as a low-level transport/testing constructor;
runtime *_STORAGE_TOKEN configuration is rejected.

Per process, one async mutex coalesces login/refresh. Access/rotating refresh tokens
stay in memory; restart logs in fresh. Refresh happens on use, up to 60 seconds early,
using both monotonic and wall-clock expiry for suspend/clock-change handling. A
400/401 refresh rejection permits one password reauthentication. Failure backs off
for five seconds. Storage 401 invalidates only the matching cached token, so stale
responses cannot evict a newly refreshed session. Storage writes are not replayed.

Requests reject redirects, use bounded connect/total timeouts and cap Auth bodies at
64 KiB. Tokens/headers are size/format checked; TTL is 1–86400 seconds. Credentials
have no Debug/Serialize implementation and public errors contain no provider body.
Authorization/apikey headers are marked sensitive. Technical sends reject a different
origin. No tokens are written to disk or passed to the engine child.

The permission boundary is the reviewed Supabase bootstrap/RLS registry, not the
account's email label. Reader accesses active published files; worker accesses live
attempt writes and policy-eligible cleanup. Ordinary humans, unrelated buckets,
overwrite and deletion of active published objects deny. Disabling a principal
revokes RLS access independently of JWT expiry; rotate to a new UUID after compromise.

Use [SUPABASE_SETUP.md](../SUPABASE_SETUP.md). The policy bootstrap is provider-specific
operator work after migration 0016, not a general Postgres migration or automatic
startup hook. Local HTTP and PostgreSQL policy tests do not claim a live Supabase
JWT/gateway/Storage acceptance result.
