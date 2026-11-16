# Hosted API OpenAPI contract

`schemas/http-v1.openapi.json` is the repository's OpenAPI 3.1 contract for all
currently registered hosted routes. It is an exported artifact, not a new public
HTTP endpoint. The historical filename of this document is retained for links.

Coverage includes:

- Identity, explicit personal onboarding, project creation and detail.
- Job admission, status, cancellation and redacted attempt diagnostics.
- Cursor-based project, job, revision and admitted-catalog discovery.
- Catalog detail, intent readiness validation and verified artifact download.
- Admin overview, user/tenant/audit metadata, catalog validation/promotion/admission,
  confirmed suspension and scoped operational job inspection/cancellation.
- Public process liveness and dependency readiness.

JSON response envelopes reject unknown properties. UUIDs, dates, hashes, nullable
heads/revisions/cursors and job states are modeled explicitly. UUID/hash pages use
`after`; newest-first audit pages use positive int64 `before`. Page limits are
1–100, default 25. Audit IDs and returned audit cursors are decimal strings to avoid
JavaScript integer precision loss. Query cursors remain subject to server parsing
and int64 bounds. Pages are not transactional snapshots.

Business operations inherit bearer authentication. Admin operations additionally
carry `x-required-role: admin`; this documents the current database registry check,
not a client-supplied claim. Admin access does not grant private project/artifact
access. Mutation request schemas require confirmation and expected-state fields;
matching the path target and compare-and-set semantics remain server checks.

Project creation/generation require an Idempotency-Key. Generation requires an
explicit nullable expected-head precondition. Intent/options/catalog/requirements
schemas reuse engine definitions with local references rebased for OpenAPI. A valid
schema never substitutes for authorization, feasibility, name whitespace/control
checks, resource limits or engine semantic validation.

Success payloads are typed except artifact downloads: the allowlisted filename
selects the actual JSON payload. That endpoint deliberately describes the JSON
attachment and integrity/cache headers without claiming one common artifact shape.
Use the engine artifact schemas/package manifest and verify digests. Error envelopes
are typed; `error.details` remains extensible for clarification/engine evidence.
Endpoint Markdown contracts describe error-code/status semantics. Unknown routes and
unsupported methods still return structured errors; they are not exported as routes.

Regenerate after contract changes, before running tests:

```sh
cargo run -q -p labyrinth-api --example export_openapi --locked > schemas/http-v1.openapi.json
cargo test -p labyrinth-api --test openapi --locked
```

Contract tests enforce export freshness, route coverage, unique operation IDs,
local schema resolution, strict authority boundaries, confirmation requirements,
nullable head preconditions and metadata privacy. PostgreSQL-backed HTTP tests
validate real response payloads against the matching operation/status schema with
UUID/date format checks enabled. This runs inside the existing integration suite;
it does not replace live provider acceptance or byte-level download integrity tests.
