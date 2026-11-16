# Public contract schemas

Generated from Rust types: intent, network, addressing-policy, logical-policy,
generation-options, verification-report, package-manifest and generation-error.
The catalog schema lives in `catalog/schemas/`.

Run `cargo run -p labyrinth-engine --example export_schema -- <name>` to print a
schema. Replace checked-in files only after successful export. Freshness and fixture
compatibility are enforced by `cargo test -p labyrinth-engine --test schemas`.
Schemas supplement semantic/cross-reference validation; they do not prove feasibility.
See [core contracts](../docs/contracts/core-v1.md). Authenticated HTTP contracts are documented in [HTTP V1](../docs/contracts/http-jobs-v1.md); OpenAPI export remains a later integration task.

`http-v1.openapi.json` covers the hosted API's registered routes, JSON request and
response envelopes, pagination, admin role boundary and download headers. Regenerate
with `cargo run -q -p labyrinth-api --example export_openapi --locked` and consult
[the contract scope](../docs/contracts/openapi-admission-v1.md) before generating a
client: downloaded artifact types and engine error-detail variants remain separate.
