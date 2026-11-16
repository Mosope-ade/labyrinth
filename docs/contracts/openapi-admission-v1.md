# Admission OpenAPI export

`schemas/http-v1.openapi.json` is an OpenAPI 3.1 admission **request subset**.
It describes personal onboarding, project creation, generation admission and health
checks. It is a repository artifact, not a new public HTTP endpoint. It does not yet
provide typed success responses or describe discovery, downloads or admin endpoints;
use their existing Markdown contracts. Do not generate a complete client SDK from
this subset.

Requests use bearer authentication except health checks. Tenant/project UUIDs are
path parameters; clients cannot provide actor identity or roles in request bodies.
Project creation and generation require an Idempotency-Key. Generation requires an
explicit nullable expected-head precondition. Intent and generation-option schemas
are generated from the engine contracts with local references rebased for OpenAPI.
Schema validity alone does not imply feasibility or replace authorization, capacity,
name whitespace/control-character checks or engine semantic validation.

Regenerate after contract changes:

```sh
cargo run -q -p labyrinth-api --example export_openapi --locked > schemas/http-v1.openapi.json
cargo test -p labyrinth-api --test openapi --locked
```

Tests enforce export freshness, unique operation IDs, local schema resolution,
unknown authority-field rejection and the required nullable head precondition.
