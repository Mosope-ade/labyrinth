# Public contract schemas

`intent-v1.schema.json` and `network-v1.schema.json` are generated from engine Rust
types. Catalog schema lives in catalog/schemas/. Freshness and fixture compatibility
are enforced by `cargo test -p labyrinth-engine --test schemas`.

Regeneration commands and semantic-validation limits are documented in
[engine contracts](../docs/contracts/engine-v1.md). Schemas do not replace cross-field
validation. No HTTP/OpenAPI schema or generation endpoint exists yet.
