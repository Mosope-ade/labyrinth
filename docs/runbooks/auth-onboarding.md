# Authentication and onboarding setup

1. Use a separate Supabase project/environment. Configure asymmetric signing keys;
   LABYRINTH_AUTH_ISSUER must be that project's exact HTTPS /auth/v1 issuer. The API
   must not use migration-owner/database service privileges at runtime.
2. Configure Supabase email magic links and PKCE-compatible templates following
   [passwordless email](https://supabase.com/docs/guides/auth/auth-email-passwordless)
   and [PKCE](https://supabase.com/docs/guides/auth/sessions/pkce-flow). Configure
   exact environment-specific Site URL/callback allowlists; avoid wildcard production
   redirects. Configure production SMTP and provider abuse/rate controls before launch.
3. GUI/CLI authentication clients use the public project URL/publishable key and
   PKCE. They initiate signInWithOtp with an allowed callback, retain the verifier
   securely, exchange the returned code through Supabase and use the access token
   for resource API calls. They own refresh/logout through Supabase. Do not send
   verifier, refresh token, callback tokens or emails to application logging.
   The CLI implementation is documented in [CLI usage](../../cli/README.md);
   the GUI client remains pending. No backend auth proxy or CORS change is added.
4. Apply migrations 0009 and 0010 using the migration owner. Configure
   LABYRINTH_TENANT_MAX_PROJECTS=50 and LABYRINTH_ONBOARDING_MAX_ACTIVE_JOBS=2 on the API.
   The existing separate runtime login remains limited to labyrinth_api.
5. With an actual provider-issued access token, GET /v1/me. For a new identity this
   reports provisioned=false and does not create application rows. Explicitly POST
   {} as application/json to /v1/onboarding/personal-workspace. Save the tenant_id
   returned; repeat safely after an uncertain response.
6. POST {"name":"First design"} to /v1/tenants/{tenant_id}/projects with a valid
   Idempotency-Key and bearer token. Save project_id, then use the existing structured
   generation/job/status/revision/artifact APIs. A new project has no head revision.

Acceptance before hosted release: deliver a real magic link, reject unallowlisted
callbacks, exchange it once with the matching verifier, reject verifier/code replay,
exercise refresh rotation/logout, complete explicit onboarding twice with one tenant,
and create/submit/poll/download a design. Test suspension and membership revocation.
Record provider settings and deployment evidence without storing secrets or raw links.

Local unit/integration tests cover signed bearer verification and transactional
onboarding/project creation. They do not claim these live provider/client checks.
No Supabase settings, emails or hosted application resources are changed automatically.
