# V1 core acceptance — 2026-11-12

Scope: pure Rust engine M0–M3. This is not hosted-backend or full-product release
acceptance. No AI key, network service, database, authentication or object storage is
needed by the engine tests or complete-package example.

## Requirement-to-evidence audit

- Intent/model/catalog: `intent`, `model`, `catalog`, `schemas`, `wan` tests cover
  unknown versus explicit inputs, strict validation, defaults/provenance, immutable
  validated types, generated schema freshness and catalog identity. WAN connectivity
  requires explicit reciprocal service declarations; Tier3 needs confirmed services.
- Selection/blueprints: `selection`, `blueprints`, `traffic`, `sessions`, `artifacts`
  cover lexicographic priorities, independent exhaustive small ranking/port references,
  capability/PoE/media limits, growth once, exact session packing references, repair
  bounds and exclusion of cheaper profiles that fail post-failure capacity.
- Physical/logical assembly: `topology`, `ipam`, `routing` cover explicit physical
  assignments, VLAN/subnet/transit allocation, LACP/RSTP forwarding, conservative HA
  ownership/sync/path failure and destination next-hop trees over explicit WAN transport.
- Five-pass assurance: `verification`, `policy`, `sessions` and internal verifier
  mutation tests cover structure, addressing, routing, deny-overrides/default-deny
  isolation, capacity, session conservation, failures and budget rejection. Removing
  policy bindings/routes or corrupting addressing cannot obtain passing verification.
  Shared handoffs cannot multiply committed WAN bandwidth. Service equivalence classes
  include all IP protocol numbers and TCP/UDP port intervals, not sampled services.
- Resilience: `scenarios` generates Tier0/1/2 multi-branch designs and Tier3 with two
  hubs/two branches. Every declared site/provider failure is checked, including
  surviving branch traffic. Link/device protection and endpoint/site exclusions appear
  in reports; no promise covers equipment or domains outside the modeled protected set.
- Package/reproducibility: `artifacts` verifies all 13 files, content hashes/sizes,
  BOM quantity reconciliation, repeated generation and input-order invariance. Private
  `VerifiedPackage` construction prevents bypassing the complete verifier.
- Incremental context: `incremental` covers endpoint and lexically earlier new-site
  additions, every prior CIDR/VLAN/core mode pin, insufficient subnet rejection,
  immutable bases, stale/omitted pins, catalog drift and modified saved bytes. Saved
  package integrity is anchored to an authoritative expected manifest digest supplied
  by the caller; the core does not authenticate that caller.

## Executed gates

Rust 1.95.0, Linux, debug profile, locked dependencies:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p labyrinth-engine --example generate_fixture --locked
```

The workspace run passed 124 tests including five compile-fail doctests; zero failed
or ignored. Schema freshness checks compare all generated contracts with Rust types.
The example prints the manifest only after generating and verifying the complete
package. Intermediate `allocate_fixture` and `assemble_fixture` examples remain
explicitly unverified stage demonstrations.

Scale regression: 126 selected devices and 496 failure cases completed the complete
five-pass/package path. A separate observed debug run took 20.498 seconds total;
this is a local observation, not a production SLO. The complete workspace run also
passed this scenario. Selection retains its 15-second cooperative search deadline;
the complete job contains additional allocation, independent proof and serialization.
CI runs the same formatting/target/lint/test gates in `.github/workflows/engine.yml`.
Local command logs are ignored diagnostic files, not required runtime dependencies.

## Assurance limits and remaining milestones

Only the first hub-spoke blueprint is implemented; campus/branch are validation
scenarios, not the product boundary. Other explicit blueprints reject. IPv4,
directional stateless policies and controlled session steering are bounded abstractions.
No NAT, implicit connection tracking, IPv6, live protocol convergence, real carrier
underlay proof, vendor configs, virtual labs or real-device certification is claimed.
HA/RSTP proofs concern the modeled steady state and declared failure cases.

Search limits can produce exhaustion or timeout without a package. Exact ranking is
within fixed enumerated candidates/port plans, not global topology optimality.
Incremental V1 supports additive changes with immutable prior allocations/core modes;
unsupported destructive changes require explicit redesign. See the
[core contract](../contracts/core-v1.md) and
[verification contract](../contracts/verification-v1.md) for bounds and assumptions.

M4+ must still supply tenant authorization, private storage, authoritative revision
metadata, durable asynchronous jobs, hard worker CPU/memory/time enforcement,
cancellation/recovery, deployment/retention and operational security testing. CLI,
AI integration, GUI and admin acceptance remain separate. Core acceptance does not
claim those unfinished product requirements have passed.
