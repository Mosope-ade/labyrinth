# Authentication and personal workspace onboarding V1

V1 interactive login uses Supabase email magic links with PKCE. The GUI/CLI auth
client owns the verifier, code exchange and refresh session; the Rust API receives
only access bearer tokens and uses its existing asymmetric JWT verification. Login
and refresh use Supabase Auth directly, not new password/token proxy routes. Browser
and CLI auth interfaces are implemented in their respective later milestones.

`GET /v1/me` requires a valid access token. It reports user_id, provisioned and
personal_tenant_id (nullable). Reading identity or logging in creates no application
rows. A suspended application user gets 403. Non-anonymous verified provider sessions
may explicitly request application onboarding.

`POST /v1/onboarding/personal-workspace` accepts only `{}` as JSON, capped at 1 KiB.
No actor, role, tenant or quotas may be supplied. Actor identity comes from the JWT.
The transaction creates an application user if absent, one personal tenant and its
owner membership. A permanent user-to-personal-tenant mapping enforces at most one
personal workspace even across concurrent requests/restarts. First creation returns
201; replay returns 200 with tenant_id, role, max_active_jobs, max_projects and replayed.

Initial explicit configuration: LABYRINTH_ONBOARDING_MAX_ACTIVE_JOBS=2 and
LABYRINTH_TENANT_MAX_PROJECTS=50. Both are required to enable onboarding; absence of
the optional onboarding setting leaves the route unavailable (503). Malformed or
partially configured onboarding policy fails startup. Quotas are persisted on the
new tenant. Existing tenant job quotas are not rewritten. Legacy tenants without a
persisted project cap use the explicit configured project policy when creating.

Suspended users/tenants and revoked memberships cannot replay onboarding to gain
access or create another personal tenant. A replay never restores membership or
promotes roles. No administrator role is provisioned. The API does not store emails,
passwords, PKCE verifiers, refresh tokens or provider user metadata.

Configure Supabase Auth email delivery, confirmation, exact HTTPS callback allowlists
and PKCE-compatible link templates before hosted acceptance. Do not accept arbitrary
redirect URLs or put access/refresh tokens in application URLs or logs. Persist the
PKCE verifier securely in the initiating client and remove the one-use callback code
from the displayed URL after exchange. Browser login/refresh and CLI browser handoff
are client responsibilities; scripted credentials still need a separate contract.

Refresh-token rotation and logout occur through Supabase. Already-issued access JWTs
may remain valid until expiry; current application suspension/membership revocation
still takes effect at each database operation. No live email delivery or Supabase
project configuration is claimed by local signed-token/JWKS tests.

References: [email passwordless/PKCE](https://supabase.com/docs/guides/auth/auth-email-passwordless),
[PKCE flow](https://supabase.com/docs/guides/auth/sessions/pkce-flow),
[sessions and refresh](https://supabase.com/docs/guides/auth/sessions).
