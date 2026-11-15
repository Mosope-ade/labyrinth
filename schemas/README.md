# Public contract schemas

Generated from Rust types: intent, network, addressing-policy, logical-policy,
generation-options, verification-report, package-manifest and generation-error.
The catalog schema lives in `catalog/schemas/`.

Run `cargo run -p labyrinth-engine --example export_schema -- <name>` to print a
schema. Replace checked-in files only after successful export. Freshness and fixture
compatibility are enforced by `cargo test -p labyrinth-engine --test schemas`.
Schemas supplement semantic/cross-reference validation; they do not prove feasibility.
See [core contracts](../docs/contracts/core-v1.md). HTTP/OpenAPI is a later backend task.
