# L2 forwarding, HA ownership and canonical assembly — V1

Implemented 2026-11-12. This stage assembles structurally validated canonical entities.
It does not synthesize routes/firewall policy or produce a five-pass verified package.
See [addressing-v1](addressing-v1.md) for its immutable allocated input.

## Public stage boundaries

- `topology::l2_forwarding(allocated, failures, preferred_roots)` returns abstract
  steady-state Ethernet trees, blocked cables and unreachable surviving devices.
- `topology::ha_ownership(allocated, failures, logical_policy, previous_owners)`
  returns one active owner or an explicit unavailable/sync-lost result per zone.
- `topology::assemble(allocated, catalog, logical_policy)` creates and structurally
  validates a baseline `NetworkDraft`, returning `ValidatedNetwork`.
- `pipeline::assemble_network` combines allocation and baseline canonical assembly.

`LogicalPolicy` permits per-zone distribution-root and firewall-owner preferences;
empty preferences choose stable IDs. Unknown zones, devices, failures and invalid
role/zone preferences reject. The public policy schema is
`schemas/logical-policy-v1.schema.json`. Failure inputs are confirmed device/cable IDs,
not inferred silence, heartbeat timers or a discovery/fencing service. They are bounded
against the allocated graph before evaluation. These functions do not mutate the input.

## Ethernet forwarding contract

Each blueprint zone declares its Ethernet cables. Parallel cables between a device
pair become a single logical LACP adjacency; canonical assembly creates an explicit
LACP bundle and checks both selected operating modes. Each individual live member
is retained until it fails. One member loss does not remove a surviving adjacency.
LACP is not modeled as the sum of member rates for an individual flow.

Choose the preferred surviving distribution root, otherwise the lowest surviving
distribution ID. Run deterministic breadth-first traversal over device-pair order.
The first edge reaching a device joins the tree; remaining live adjacencies are
blocked. Expand logical adjacencies back into their physical members for evidence.
Failed devices/cables are absent from both forwarding and blocked lists. Surviving
unreachable devices are listed explicitly; loss of all roots gives no forwarding root.

The baseline tree spans every domain participant and is loop-free after collapsing
validated LACP bundles. Cyclic Ethernet topologies require RSTP capability on the
switching participants. This is an abstract stable forwarding model, **not** an
implementation of RSTP bridge IDs, path-cost selection, timers, BPDU handling, MAC
learning, convergence loss, STP interoperability or vendor behavior. Tree selection
uses hop count and stable IDs, not line-rate optimization. Failures recompute a new
steady state; there is no claim of hitless failover or post-failure capacity yet.

The first adapter has distribution peers, dual-homed access and leaf firewall
attachments. Firewalls do not bridge independent physical attachments. Tier1 Ethernet
members use LACP; Tier2/3 redundant switching uses an abstract tree and explicit RSTP
capability. Routed ECMP groups remain distinct from Ethernet LACP groups.

## Active/standby ownership contract

The user approved conservative active/standby ownership, deterministic overridable
preference, no automatic preemption, and fail-closed behavior on total HA-control loss
while both peers remain alive. Confirmed owner-device failure permits takeover.

`blueprint_rules_v3` reserves **two separate physical HA-control cables** per firewall
pair; v2 reserved one and could not survive its loss. A single surviving control cable
preserves synchronization availability in this bounded model. This is cable redundancy,
not proof of independent conduits, power or real protocol synchronization performance.

A standalone gateway has one member address and no VIP/control links. An HA gateway
uses the first two reserved addresses for member interfaces and the third for its
virtual address. Only the owner may forward as the virtual gateway; synthesis and
verification must honor this ownership rather than treating both members as active.
Extra configured reservations remain unassigned. No addresses are shared by individual
physical interfaces or duplicated across members.

Without history, select an eligible preferred owner then stable member ID. An eligible
member is confirmed alive, reachable in its zone's recomputed L2 tree, and has at least
one live local routed transit attachment. This local check does not prove remote
routing/ISP reachability. With a previous owner, retain it while eligible; a recovered
preferred peer cannot preempt. The caller must preserve the last committed owner
across evaluations; omitting history intentionally requests a new stateless election.
If both members remain alive and all control cables fail, return no owner with
`sync_lost`. With a confirmed dead owner, the surviving eligible peer may take over
without a live cable to the dead peer. If no candidate is eligible, return unavailable.

The user also approved coordinated path-health transfer: when a live previous owner
loses L2 reachability or every local routed transit attachment, a healthy HA-control
path permits transfer to an eligible standby. The previous owner is not treated as
dead; healthy synchronization is the coordination assumption. Losing all control
paths while both peers remain alive still fails closed, even if the old data path
is also broken. With no eligible standby, ownership is unavailable. After a committed
transfer, recovery of the old preferred owner does not preempt the new owner.

This monitors only the explicitly modeled local L2/transit paths. It does not detect
remote ISP blackholes, measure performance, implement an ownership handshake or
provide real fencing/quorum. Resilience verification must still expose unsupported
failure cases rather than grant Tier2 or Tier3 assurance from this function alone.

## Canonical representation and validation

Canonical assembly contains sites/user/infrastructure zones, all selected devices,
physical port bindings, internal cables, LACP/ECMP groups, VLAN domains, IP allocations,
member gateway interfaces/VIPs, HA control paths, L2 states and decision evidence.
Site failure IDs are distinct from ISP/provider domains. Provider handoffs remain
explicit unaddressed physical `provider` interfaces; no remote nodes, carrier links
or provider addresses are fabricated. Route and policy collections remain empty.

New interface kinds are explicit:

- `attachment`: physical routed-device Ethernet termination of a zone domain; does
  not require firewall switching or imply firewall bridging.
- `gateway`: logical routed address over that device's single attachment/LACP bundle.
- `ha_control`: physical reciprocal peer control channel, not ordinary /31 data transit.
- `provider`: unresolved external handoff with its declared provider failure domain.

The validator checks interface/media/catalog capabilities, reciprocal HA peers,
Ethernet domain agreement, per-device LACP member profiles, distinct addresses,
reserved gateway space, gateway member/owner references, VIP uniqueness, control links,
complete baseline L2 cable classification, loop freedom and spanning reachability.
Old network-v1 fixtures without the additive forwarding/gateway arrays default them
to empty. A legacy draft's structural validity is not evidence that it has those proofs.

Physical interface admission remains 256/device; up to 16 additional logical
interfaces are admitted separately (272 total/device), with 65,536 total interfaces.
Forwarding state admits at most 65,536 cable entries in total. Logical gateway
membership is bounded to one/two members and 32 control paths. Catalog and address
pins remain exact; full incremental revisions still require a later context contract.
Loopback-enabled allocation requires routing in every selected mode before selection,
so a pure L2 mode is not silently promoted to a routed device during assembly.

## Reproduce

```sh
cargo run -p labyrinth-engine --example assemble_fixture --locked
cargo test -p labyrinth-engine --test topology --test model --locked
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Tests cover tree cycles/blocked edges, root loss, LACP member loss, HA-control loss,
owner failure, recovery without preemption, preferences and invalid references,
Tier0–Tier3 canonical role shapes, input-order invariance, JSON roundtrip and malformed
canonical gateway/forwarding state. Tier3 shape coverage is not site-failure sign-off.
