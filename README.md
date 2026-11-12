# Labyrinth

From intent to infrastructure.

Labyrinth is a vendor-neutral network topology generator. Its deterministic Rust
engine will produce topology, IPAM/VLAN plans, abstract routing/firewall policies,
an archetype BOM and verification evidence. AI only parses intent and optionally
assesses completed designs. CLI and GUI will use the same hosted API.

## Current state

The first engine contract slice is implemented: partial/resolved intent, typed
network drafts, structural integrity checks, catalog YAML/schema/semantic validation,
immutable catalog identity and fixtures. Deterministic role planning, capability filtering and bounded assignment ranking
are implemented too. Physical port binding and configurable IPv4/VLAN allocation
now produce an intermediate topology plan. Logical forwarding, five-pass verification,
running services and CLI commands are not implemented yet.
The previous code has not been restored. Campus/branch WAN are validation scenarios,
not the product's scope. Vendor configs and runnable labs are outside V1.

See [engine contracts](docs/contracts/engine-v1.md) for exact semantics, admission
limits, schema tooling and assurance boundaries.

## Source map

- `engine/`: pure Rust library; typed intent, canonical model, YAML catalog ingestion,
  blueprints, selection, addressing, routing, policy, verification and artifacts.
- `api/`: hosted API application boundary; auth, projects, jobs and admin operations.
- `worker/`: durable job execution and artifact publication; depends on the engine.
- `cli/`: hosted client boundary for `laby`; no engine dependency or offline generator.
- `web/`: future React/TypeScript workspace and admin GUI, after backend acceptance.
- `catalog/`: versioned archetype YAML, schemas and catalog bundle metadata.
- `schemas/`: future public intent/model/API contract outputs.
- `tests/`: cross-component tests, scenario fixtures and independent reference models.
- `infra/`: future deployment/local service definitions; no providers selected yet.
- `docs/`: architecture decisions and contract documentation.

API, worker and CLI currently have library targets only. Runnable entrypoints will
be added when they have real behavior; the CLI executable will be named `laby`.
Engine integration tests belong in `engine/tests/`; shared fixtures live in
`tests/fixtures/`. The engine must not depend on infrastructure or either client.

## Checks

From the repository root with a Rust toolchain supporting edition 2024:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Engine fixtures need no AI key, database or services. Cargo dependencies are pinned
in Cargo.lock. CI runs formatting, target checks, Clippy, contract/fixture tests and
schema/catalog freshness checks. Passing these does not claim a working generator.

## Product and engineering contracts

Read [PRD.md](PRD.md) for requirements and [plan.md](plan.md) for milestones.
Read each boundary's documentation before adding dependencies. Keep contracts typed,
errors explicit, resource use bounded and changes tested. Follow the documents'
security, scalability, reliability and code-review requirements from the first slice.

Next: resolve L2/HA forwarding, assemble the complete canonical topology, and
implement abstract routing/policy synthesis with five-pass verification. Full
incremental blueprint planning also awaits explicit prior-revision context.
See [selection contracts](docs/contracts/selection-v1.md) for scoring, repairs and limits.

See [addressing contracts](docs/contracts/addressing-v1.md) for configurable conventions,
allocation guarantees and the intermediate plan demonstration.
