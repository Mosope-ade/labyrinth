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

Intent, model and catalog now have typed contracts and positive/negative tests.
Selection, hub-spoke blueprint rules and the first pipeline coordinator slice are
also implemented. Physical port binding and IPv4/VLAN allocation are implemented;
logical forwarding, route/policy synthesis and verification remain future work.
Draft/model readiness and selected role plans are not five-pass
verification. See ../docs/contracts/engine-v1.md for supported semantics and limits.
No HTTP, storage, AI or UI dependencies.

See [addressing-v1](../docs/contracts/addressing-v1.md) for the intermediate output,
configurable defaults and developer example. Full incremental planning is still gated
on prior-revision context even though the allocator can preserve individual pins.
