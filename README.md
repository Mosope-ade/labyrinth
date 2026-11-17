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
storage adapters and retention are implemented and locally/CI tested. Live provider
acceptance, product CLI, optional AI clarification, GUI and admin UI remain unfinished.

## Source map

- `engine/`: pure Rust library; typed intent, canonical model, YAML catalog ingestion,
  blueprints, selection, addressing, routing, policy, verification and artifacts.
- `api/`: hosted API application boundary; auth, projects, jobs and admin operations.
- `service-auth/`: server-only technical identity login/refresh for scoped Storage.
- `worker/`: durable job execution and artifact publication; depends on the engine.
- `cli/`: hosted client boundary for `laby`; no engine dependency or offline generator.
- `web/`: future React/TypeScript workspace and admin GUI, after backend acceptance.
- `catalog/`: versioned archetype YAML, schemas and catalog bundle metadata.
- `schemas/`: generated public intent/model/options/report/package contracts; API contracts follow.
- `tests/`: cross-component tests, scenario fixtures and independent reference models.
- `infra/`: deployment/local service definitions follow ADR 0003; no hosted services provisioned.
- `docs/`: architecture decisions and contract documentation.

API and worker have runnable entrypoints and Docker packaging. The source-built CLI executable is `laby`; see [CLI usage](cli/README.md).
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
schema/catalog freshness checks. Hosted provider acceptance is separate.

## Product and engineering contracts

Read [PRD.md](PRD.md) for requirements and [plan.md](plan.md) for milestones.
Read each boundary's documentation before adding dependencies. Keep contracts typed,
errors explicit, resource use bounded and changes tested. Follow the documents'
security, scalability, reliability and code-review requirements from the first slice.

Start with [Docker setup and next steps](docs/runbooks/docker-local.md). Prepare
your free backend services using [SUPABASE_SETUP.md](SUPABASE_SETUP.md).
The root environment template has been removed; component templates live under
api/ and worker/. Do not commit local secrets.
The core itself requires no AI key, HTTP, database or services.

Hosted API now includes SQLx migrations, atomic PostgreSQL/pgmq admission,
Supabase asymmetric token verification and authenticated Axum submission/status.
See [API setup and tests](api/README.md). Provider provisioning and live acceptance,
Live CLI/browser-provider acceptance and GUI delivery remain unfinished.

The source-built [laby CLI](cli/README.md) now supports magic-link PKCE login,
secure refresh/logout and explicit identity/workspace/project commands. Live
Supabase acceptance waits for provider setup; the GUI is not implemented yet.
