# Physical binding and addressing contracts — V1

Implemented 2026-11-12. This is an intermediate engine stage, not a complete network
or verification report. `pipeline::allocate_topology` runs deterministic selection,
binds installed catalog ports to physical links/endpoint attachments/provider
reservations, and allocates IPv4/VLANs. No AI, database, network or services are needed.
`AllocatedTopology` has no public constructor or deserializer and exposes immutable
views. It cannot be substituted for `ValidatedNetwork` or a verified package.

## Topology-independent allocation

`ipam::allocate(AddressRequest)` accepts named subnet requests, a canonical pool and
exclusions. Each request declares its zone, purpose, required/reserved host counts,
optional exact prefix and optional pinned subnet. Counts must already include desired
growth: this allocator never reapplies it. Multiple requests per zone are supported.
The initial routing realm has global IPv4 uniqueness; overlapping VRFs are not modeled.

All pins are validated and reserved before free allocations. Pins must fit the pool,
capacity, optional exact prefix, and avoid exclusions/other pins. A larger pinned
subnet is preserved if no exact prefix was requested. Pins never move automatically.
This primitive does not authenticate a base revision or implement full incremental
updates. The blueprint coordinator still rejects updates lacking prior-revision context.

Free allocations are sorted by largest block first, then stable allocation ID. For
each request choose the lowest aligned gap. Occupied intervals are skipped directly;
no iteration over the addresses of a /8 or /0. Nested/overlapping exclusions are
allowed, including exclusions containing the entire pool; outside exclusions have
no effect. Results are sorted by allocation ID and independent of request order. The result
records addressing_rules_v1 plus canonicalized requests/pool/exclusions for replay.

Endpoint subnets use network/broadcast semantics and at least /30. Transit/loopback
requests permit /31 and /32 host semantics. An exact prefix must fit all required and
reserved hosts. Default minimum size is determined from usable capacity, not an
arbitrary /24. Reserve low usable addresses first, followed by a contiguous required
host range. Ranges store first/last/count instead of materializing every endpoint.
Unassigned usable capacity is spare space, not an implicit host assignment.

Errors distinguish invalid requests, conflicting pins, subnet exhaustion (allocation
ID, prefix and pool), and VLAN exhaustion. Failure returns no partial plan. Exhaustion
is failure of this deterministic strategy; it is not a general packing/optimization
proof. Requests are limited to 1–4096 subnets and 256 exclusions before allocation.
IPv4 boundary arithmetic uses u64 so the end of 255.255.255.255 never wraps.

`ipam::allocate_vlans` accepts arbitrary site/zone requests, optional pins and a
`VlanPolicy`. Reserve pins first, then assign the lowest available tag in sorted
site/zone order. Uniqueness can be per site or global. Bounds must lie in 1–4094;
exclusions and pins are enforced, duplicate site/zone requests reject. Maximum 4096
requests and 4094 excluded IDs. Per-site reuse requires broadcast-domain boundaries;
this allocator alone does not prove those boundaries.

## Configurable blueprint addressing policy

The user confirmed the conventions below **as configurable defaults**, not universal
topology rules. `topology::AddressingPolicy` is an explicit pipeline argument, serialized
inside every allocated result. Future blueprints may construct different subnet and
VLAN requests without changing the general allocator.

The strict policy JSON Schema is `schemas/addressing-policy-v1.schema.json`. Runtime
checks additionally enforce relational constraints (HA reservation minimum, VLAN
range order and exclusions). Regenerate using `export_schema addressing-policy`.

Default first hub-spoke adapter:

- One endpoint subnet/VLAN per site security zone; all access partitions share it.
- Tier0/1 reserve one gateway address; Tier2/3 reserve three (two firewall addresses
  and a shared virtual gateway). These reservations are consumed by the separate HA ownership stage.
- Gateway reservation counts are configurable positive u16 values; the current HA
  adapter requires at least three reservations.
- Routed internal links request /31s; `transit_prefix_len` can choose a larger subnet
  (0–31, capacity/pool permitting). /30 uses its two ordinary usable hosts.
- Allocate one /32 per selected device; loopback allocation can be disabled.
- VLANs start at 10, end at 4094, permit reuse across sites, and have no extra exclusions.
  Start/end, exclusions and uniqueness scope are configurable.
- IPv4 pool and excluded prefixes come from resolved intent. Its visible default pool
  remains 10.0.0.0/8. Endpoint totals are taken from grown access populations, not grown
  a second time. Gateway reservations are additive infrastructure needs.

Stable derived IDs associate endpoint/transit/loopback allocations with their source
zone/link/device. Endpoint ranges reserve capacity for grown endpoint populations;
they are not discovered MAC/IP inventories. Generated infrastructure-zone IDs hold
transit/loopback allocations separately from user zones. They confer no permission,
routing reachability or implicit policy. Derived collisions with user zone IDs reject.

## Physical output and assurance boundary

Every selected port assignment becomes a distinct `BoundPort` with a catalog group,
zero-based index, configured speed/media and stable interface ID. Link endpoints
reference these exact ports. Endpoint and provider ports are explicitly distinguished;
provider reservations do not fabricate remote devices, carrier paths or provider IPs.
`PhysicalLink` records are the existing canonical link type, but no complete
`NetworkDraft` is emitted at this stage.

Blueprint-declared L2 links and firewall HA-control cables are excluded from ordinary
routed transit allocation. Other current blueprint links receive transit allocations;
loopbacks and endpoint/gateway ranges are recorded separately. The separate canonical assembly stage resolves L2 aggregation, HA ownership,
loop prevention and routing/policy synthesis; see [core contract](core-v1.md).
The user selected zone distribution switches for shared-VLAN aggregation. Blueprint
rules v2 implement that role layer, LACP/RSTP capability requirements, dedicated
firewall HA sync links and explicit L2/control-link classification. Active/blocked L2 forwarding, bundle realization and HA state/ownership are
implemented in the subsequent stage. Physical
cables and protocol capabilities alone do not establish those proofs; this intermediate
output is intentionally not export-eligible.

## Reproduce and verify

```sh
cargo run -p labyrinth-engine --example allocate_fixture --locked
cargo test -p labyrinth-engine --test ipam --test topology --locked
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The example uses synthetic fixture hardware and prints an intermediate JSON plan.
It is developer tooling, not an offline product CLI. Tests cover explicit policies,
exact catalog port bindings, no second growth, deterministic ordering, pin conflicts,
nested exclusions, fragmented/full pools, VLAN scopes, /31-/32 semantics, full IPv4
boundaries and comparison against an independent small first-fit reference model.

## Subsequent logical stage

[logical-v1](logical-v1.md) now defines the separately implemented L2/HA and canonical
assembly stage. Allocation remains an intermediate boundary; it does not itself grant
those semantics. Blueprint rules v3 supersede v2's single HA-control cable with two.
