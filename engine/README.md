# Core engine

Pure Rust, deterministic, vendor-neutral. Module boundaries:

- `intent`: partial/resolved requirements, normalization and readiness validation.
- `model`: stable network entities, traffic, failure domains and revision identities.
- `catalog`: archetype schemas, operating modes and immutable catalog validation.
- `blueprints`: scenario rules; core logic must not assume campus/branch topology.
- `selection`: role mapping, hard filtering, strict ranking and bounded repair.
- `ipam`: deterministic subnet/VLAN allocation primitives with pins and exclusions.
- `topology`: installed port binding and configurable addressing stage.
- `routing`: abstract forwarding and next-hop relationships.
- `policy`: required flows, isolation and modeled enforcement.
- `verification`: five-pass evidence and counterexamples.
- `artifacts`: immutable neutral engineering package and deterministic serialization.
- `pipeline`: orchestration and typed stage/failure boundaries.

The core now connects these stages end to end and emits a verified neutral package.
Use `artifacts::generate` for stateless designs and `generate_incremental` with a
validated `BaseRevision` for additions. No partial/unverified package is constructed.

Read [core-v1](../docs/contracts/core-v1.md) and
[verification-v1](../docs/contracts/verification-v1.md) for accepted semantics,
protected sets, assumptions and limits. Hub-spoke is the first supported scenario;
other architectures remain explicit unsupported requests until implemented.

No HTTP, persistence, authentication, AI, UI or filesystem dependencies enter the
library. Development examples and test fixtures are not the installed product CLI.

Complete AI-free package demonstration: `cargo run -p labyrinth-engine --example generate_fixture --locked`.
See [core acceptance](../docs/testing/core-acceptance.md) for test evidence and limits.
