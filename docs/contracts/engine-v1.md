# Engine input contracts — first implementation slice

Status: foundational contract, not a network-generation or verification implementation.

## Common rules

All input objects reject unknown fields. Stable IDs are 1–64 lowercase ASCII letters,
digits, hyphens or underscores, beginning with a letter. IDs are semantic identifiers,
not labels or randomly generated UUIDs. Numeric capacities use integer Mbps, watts,
counts or percentages; no floating-point scores or ambiguous Gbps fields.

Errors contain a stable code, JSON-pointer-style path, and actionable explanation.
Validation returns all independent issues in stable traversal order. Validated snapshots
cannot be deserialized or constructed directly. These boundary checks do not establish
feasible topology, sufficient routing or security: five-pass verification comes later.

## Catalog

Catalog YAML uses schema_version=catalog_v1, semantic version and generic archetypes.
Every field is schema-checked through strict serde types; tests also check the YAML
against generated schemars JSON Schema and assert schema-file freshness. Semantic
checks cover duplicate IDs, empty lists, positive capacities, operating-mode feature
subsets, base-mode limits, PoE consistency and breakout capacity.

Missing throughput stays null and cannot establish capability sufficiency. Shared port
capacity is an independent bottleneck, not necessarily the sum of all line rates.
Breakout describes mutually exclusive parent/child port use; it does not create extra
simultaneously usable physical ports. No vendor/SKU selector is supported.

The SHA-256 digest covers deterministic JSON serialization of the strictly parsed
bundle (including version and reference hints). YAML comments/formatting do not affect
it; array order does. This is a versioned content identity, not a cryptographic signature
or assertion that a profile's engineering numbers have been independently measured.
The caller retains exact snapshots; no filesystem/network access happens in the loader.

Loader limits: 1 MiB YAML, 256 archetypes, 32 port groups and 32 operating modes per
archetype, 32 features and 32 reference hints per archetype; ID and free-text lengths
are bounded. These are implementation admission limits, not production scale claims.

## Intent and canonical model

Partial intent preserves unresolved critical values. A complete parse is not a
resolved intent. Resolution validates references/values and either returns named
clarification fields or an immutable resolved snapshot with provenance. Invalid input
is not a clarification. Secondary defaults are recorded at the field level.

Network model contracts describe stable sites/devices/interfaces, explicit port/zone
ownership, L2 domains and parallel physical links. Draft-model validation checks
references, uniqueness, basic link compatibility and address consistency. It does not
claim reachability, feasibility, forwarding-loop freedom or any resilience proof.

Detailed traffic/policy and resolution semantics are finalized alongside their fixture
slice. Selection scoring, blueprint thresholds, generation and hosted execution remain
outside this slice. No existing project-data migration is required for this fresh engine.

### Implemented intent semantics

The user approved directional simultaneous peak demand in integer Mbps between zones.
All listed demands are concurrent; endpoint line rate is not silently substituted.
Multiple demands contribute separately. The subsequent selection/blueprint slice
implements checked growth and aggregation; see [selection-v1](selection-v1.md).
Five-pass forwarding/security verification remains separate.

Policies match directional zone pairs and protocol (any, TCP, UDP, ICMP). TCP/UDP
port ranges are inclusive (1-65535); empty ranges mean any destination port for that
protocol. ICMP/any cannot carry port ranges. There is no implicit return permit,
state tracking, ICMP-type matching, IPv6, NAT translation, rule ordering or policy
reachability solver in this slice. Default deny and actual precedence/reachability
are obligations of the future policy engine, not a result of parsing a policy list.
Required capabilities must be provided per zone, including explicit empty lists.
PoE requirements need declared positive simultaneous load. Critical unknowns clarify.

Supported resolution defaults: 20% growth, 20% spare-port target, automatic blueprint,
10.0.0.0/8 IPv4 pool and no known exclusions. Default provenance is distinct from
provided zero/empty values. Growth is currently bounded to 0-100%; spare-port target
to 0-99%. These validation ranges do not decide the unimplemented sizing formula.
Catalog throughput/port capacity and demand units all use Mbps for integer arithmetic.

Incremental input is a full replacement intent with a base revision ID, existing
zone allocation pins and core device/archetype/site pins. Both pin lists must be
nonempty. Resolve checks IDs, pin overlap, pool containment and exclusions; checking
pins against the actual prior revision/catalog and preserving assignments is later
pipeline work. Blueprint names in the contract are not claims of generator support.

Tier3 readiness requires at least two hub locations (case-insensitively distinct) and
at least two provider-domain IDs per site. Named locations/providers are supplied
claims only. Independence, site failures and failover capacity remain unverified.

### Model scope and limits

Stable IDs are typed by entity class. Link endpoints refer to a device and one of its
interfaces. Port indices are zero-based and checked against the chosen archetype/mode.
Physical links require compatible media, equal configured speeds, and usable bandwidth.
Two routed endpoints need distinct usable hosts in one subnet. Physical cables cannot
reuse endpoints; parallel cables use separate ports. Loopbacks are virtual /32s.

Initial network model supports a single IPv4 routing realm (globally unique host
addresses and nonoverlapping allocated subnets), site-local L2 domains and site-scoped
VLAN uniqueness. LACP bundles have consistent L2 memberships across one device pair;
routed aggregate interfaces and breakout child-port assignment are not yet modeled.
ECMP bundles require routed members and mode capability but do not prove equal-cost
routing. Shared-fabric/PoE/mode throughput sufficiency and failure analysis belong to
later capacity/verification passes. Draft validation does not require a complete
allocation table and does not establish a verified engineering package.

Abstract routes currently describe directly adjacent IPv4 next hops on a routed
egress; tunnel, recursive-next-hop, VRF and dynamic-protocol semantics are not implemented.
Policy bindings validate zone/interface references and match shape, not enforcement
or isolation. Decision component references are typed and checked; no wall clock or
random ID is inserted. Declared failure domains are explicit identifiers, not proofs.

JSON entrypoints reject input above 1 MiB. Intent admission: 64 sites, 64 zones/site,
4096 demands/policies, 128 ranges/policy, 1024 exclusions and 4096 pins/class. Model
admission: 256 sites, 4096 zones/devices/domains/allocations/bundles/policies, 65536
interfaces/links/routes, 256 interfaces/device and 16384 decisions. Future hosted
quotas can be lower. These are bounded admission limits, not benchmarked SLOs.

### Schema and catalog tooling

From the repository root:

```sh
cargo run -p labyrinth-engine --example export_schema -- catalog > catalog/schemas/catalog-v1.schema.json
cargo run -p labyrinth-engine --example export_schema -- intent > schemas/intent-v1.schema.json
cargo run -p labyrinth-engine --example export_schema -- network > schemas/network-v1.schema.json
cargo run -p labyrinth-engine --example export_schema -- catalog-version
cargo test -p labyrinth-engine
```

Schema freshness is enforced by tests. Schema validation checks shape/declared schema
constraints; semantic validation remains mandatory for references, CIDRs, operating
modes and cross-field requirements. Parsing directly into a resolved/snapshot type is
intentionally unsupported and covered by compile-fail documentation tests.

The reference catalog is synthetic test data, not calibrated hardware advice. Its
manifest and retained YAML snapshot are checked against the computed semantic digest.
Digests provide identity/integrity, not authenticity; signed distribution/authorization
and cross-tenant access are future API/catalog-publication responsibilities.

### Security boundary

Untrusted user/AI JSON and catalog YAML enter only bounded parsers; unknown and duplicate
object fields reject. Validated snapshots expose shared read-only access. No external
schema resolution, HTTP, filesystem loading, database, credentials or AI calls happen
in the engine loaders. CI/test schema validators have remote resolution disabled.
Tests exercise malformed input, overflow-prone catalog arithmetic, contradictory and
missing values, stale identities and unknown references. This does not establish a
security audit of future services or guarantee behavior on unmodeled networks.

Dependency APIs used: [Schemars](https://docs.rs/schemars/),
[serde_yaml_ng](https://docs.rs/serde_yaml_ng/),
[jsonschema](https://docs.rs/jsonschema/0.33.0/jsonschema/).

### Endpoint attachment extension

The user approved a default 1000 Mbps copper endpoint attachment, with explicit
overrides and field provenance. PoE zones additionally need maximum watts per endpoint
port and total simultaneous watts; inconsistent total/per-port declarations reject.
This is an additive optional input field; old complete non-PoE fixtures still resolve.
Selection/scoring and initial blueprint rules now live in selection-v1.md; they are
not generation, allocation or verification claims.


## Physical binding and IPAM extension — 2026-11-12

See [addressing-v1](addressing-v1.md) for implemented deterministic IPv4/VLAN
allocation, concrete port binding, configurable defaults and assurance boundaries.
The intermediate stage does not change NetworkDraft validation into full verification.
