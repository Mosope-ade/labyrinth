# Labyrinth

From intent to infrastructure.

Labyrinth is a vendor-neutral network topology generator. Its deterministic Rust
engine will produce topology, IPAM/VLAN plans, abstract routing/firewall policies,
an archetype BOM and verification evidence. AI only parses intent and optionally
assesses completed designs. CLI and GUI will use the same hosted API.

## Current state

The V1 core implements structured intent/readiness, versioned generic catalogs,
deterministic selection and repair, physical/L2/L3 topology, configurable IPAM/VLANs,
explicit WAN services, stateless policies, controlled session packing, five-pass
verification, neutral package assembly and context-validated incremental additions.
Campus/branch hub-spoke is the first supported scenario; other named blueprints reject
explicitly. It is not the product boundary.

See [core contracts](docs/contracts/core-v1.md) for APIs, the 13-file package,
assumptions and supported limits. Hosted API/worker execution, authentication,
storage, product CLI, optional AI clarification, GUI and admin UI remain later work.

## Source map

- `engine/`: pure Rust library; typed intent, canonical model, YAML catalog ingestion,
  blueprints, selection, addressing, routing, policy, verification and artifacts.
- `api/`: hosted API application boundary; auth, projects, jobs and admin operations.
- `worker/`: durable job execution and artifact publication; depends on the engine.
- `cli/`: hosted client boundary for `laby`; no engine dependency or offline generator.
- `web/`: future React/TypeScript workspace and admin GUI, after backend acceptance.
- `catalog/`: versioned archetype YAML, schemas and catalog bundle metadata.
- `schemas/`: generated public intent/model/options/report/package contracts; API contracts follow.
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

Next after core acceptance: hosted asynchronous backend contracts and execution,
including tenant isolation, durable revisions/jobs, storage and authentication.
The core itself requires no AI key, HTTP, database or services.
