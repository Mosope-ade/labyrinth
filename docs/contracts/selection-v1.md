# Deterministic selection and blueprint contract

Version: selection_v1 / blueprint_rules_v1. Status: implemented selection/role-planning slice; not network verification.
The user confirmed the scoring/headroom policy and default endpoint attachment.

## Settled scoring and headroom

Apply growth exactly once: `ceil(raw * (100 + growth_percent) / 100)` using checked
integer arithmetic, independently to endpoints, simultaneous peak Mbps and declared
PoE demand. Do not multiply peak traffic by endpoint growth a second time. Endpoint
port speed is a physical requirement, not a traffic-demand multiplier.

Peer groups are identical DeviceRole values across a role plan (including sites).
Rank complete feasible assignments lexicographically by:
1. Sum over peer groups of `distinct archetypes - 1` (mode differences do not count).
2. Sum of relative cost tier per installed device; a ranking proxy, not money.
3. Sum of spare-port target shortfall in parts per million.
4. Installed device count.
5. Stable sorted `(device ID, archetype ID, operating mode ID)` tuples.

At each candidate, count physical ports in catalog groups compatible with at least
one requested port class. Exclude groups unusable under the request's shared-capacity
constraint. Free fraction is `floor((installed - used) * 1_000_000 / installed)`.
Penalty is `max(0, target_percent * 10_000 - free_fraction)`. This soft target may not
outweigh standardization or cost and is not a second increase to required endpoints.
Irrelevant fiber/copper/speed groups cannot supply fictitious spare capacity.

Hard filtering checks role, selected mode features, known mode throughput, explicit
pins, physical port counts/media/speed/per-port power and known PoE budget. Missing
throughput never means unlimited. No vendor affinity or reference-hint scoring.

## Port matching and global search

Use deterministic capacitated bipartite matching across port-demand classes and catalog
port groups. Flexible groups can be reassigned; greedy first-fit cannot establish
infeasibility. Breakout child-port assignment is unsupported in this version.

For a declared shared port-group capacity, conservatively require it to accommodate
the entire device's peak if used. This can reject a feasible split-load arrangement;
report that bounded model rather than asserting hardware impossibility. A null shared
limit adds no extra group constraint; known overall mode capacity is still mandatory.

Enumerate complete fixed-topology assignments with deterministic branch-and-bound.
Input ordering and hash-map iteration cannot decide winners. Exhausting a deterministic
state budget returns SearchExhausted even if a provisional solution was seen; no
unproven winner is published. Exact ranking claims apply only when that fixed candidate
product was fully searched/pruned soundly, never globally over all network topologies.
Connections require compatible endpoint port demands and cannot exceed reserved counts.

## Blueprint scope and implementation gates

Blueprint rules produce logical roles, connections and sized requirements, not a
verified topology or allocated addresses. Core selection is generic; scenario policy
stays in the blueprint module. Explicit unsupported overrides reject, never substitute.

Automatic rule order: multiple sites choose hub-spoke; a single data-center role
recommends spine-leaf; a single site above 512 grown endpoints recommends three-tier
campus; otherwise choose a collapsed hub-spoke design. Only hub-spoke is implemented
initially. Unsupported automatic recommendations return an explicit unsupported result;
a user may explicitly request hub-spoke subject to its supported constraints. These
are versioned initial engineering rules, not universal architectural truths.

Default endpoint attachment: 1000 Mbps copper, overridable and recorded in provenance.
PoE endpoints additionally need explicit maximum per-port watts; aggregate watts alone
cannot prove port compatibility. Internal link speeds/media are chosen from common
catalog capabilities at or above the required declared load and recorded as decisions.

The role mapping, repair sequence, resource limits and outcome tests are recorded
below as each slice is completed. Routing/security/resilience proofs and IPAM allocation
remain later pipeline stages; device selection alone cannot grant verified status.

## Implemented hub-spoke role mapping

For each site, create one core at Tier0/1 or two cores at Tier2/3. Each zone gets a
firewall boundary (one device at Tier0/1, an HA pair at Tier2/3) between its access
switches and shared cores. Access/firewall/core roles require explicit forwarding and
switching capabilities; redundant paths request ECMP and HA where relevant. Required
IPS/VPN/NAT processing is assigned to zone firewall modes; PoE/LACP attach to access
roles. Actual routing and firewall policy synthesis remains unimplemented.

Tier1 requests two routed interconnects per adjacent device pair; Tier2/3 connects
access to both firewalls and each firewall to both cores. Every role/path reserves
full incident zone/site demand where traffic placement or failover distribution is
unknown. Each original demand is counted once per involved site; hubs also reserve
inter-site transit demand. Each demand is grown once before aggregation. Conservative
access reservation can exceed actual distributed load; it is not a uniform-per-endpoint
traffic assumption. Firewall boundaries are architectural requirements, not an isolation
proof or an assertion that the five verification passes have run.

WAN roles require declared provider domains for multi-site designs. Allocate at least
two WAN-edge devices at Tier2/3, cycling known providers if only one was supplied.
Provider attachment reserves one circuit at Tier0 or two at higher tiers. All cores
connect to the relevant WAN edges. External attachments are planned requirements;
carrier paths, actual circuit independence, overlays and reachability are not inferred.
Tier3 adds no false verification badge: hub/domain readiness is checked earlier and
whole-site/ISP failure verification remains a later milestone.

Choose the lowest common physical speed/media meeting declared load from catalog role
capabilities. Stable order is numeric Mbps then Media enum order (copper, fiber);
virtual media does not establish a physical interconnect. This local choice does not
exhaustively optimize link rates/media across the whole topology. Report this limit
when exhausting repairs; absence of a solution is not proof none exists elsewhere.

## Three-pass coordinator and output

Pass 1 has one access partition per zone; passes 2/3 have two/four, bounded by endpoint
count. Divide grown endpoint counts by quotient/remainder in stable device-index order;
never grow them again after splitting. Enumerate all feasible catalog archetypes/modes
at each pass, which permits equipment-tier upgrades without changing hard constraints.
Retry only a fixed-role candidate failure or interconnect feasibility issue. Return
unsupported requests, missing inputs, pin conflicts, admission limits and search limits
with their own error codes rather than disguising them as repair attempts.

Stop at the first feasible repair topology. Rank its complete assignments exactly if
the search finishes within budget; do not claim an optimal topology across repair
passes. Device-count preference is constant inside a fixed topology; repair progression
adds access devices only as needed. SelectedPlan contains assignment score, catalog
version/hash, physical port reservations, role placements, required connections,
external attachments, decisions and prior repair bottlenecks. It is NOT an allocated
NetworkDraft, verified build or complete engineering package.

Direct fixed-role selection preserves pinned archetypes and fails on incompatible
pins. Direct blueprint calls without a validated prior-revision context reject incremental
requests. Use `artifacts::generate_incremental` with `BaseRevision` for complete pinned
expansion; it never falls back to Stateless Redesign.

## Bounds, evidence and verification commands

Selection admits at most 128 devices, 64 port-demand classes/device, 256 installed
physical interfaces/device, 4096 connections and 1–1,000,000 branch-and-bound states.
The coordinator caller supplies the search budget (default blueprint request 100,000).
Oversized endpoint populations can drive repair, but device-count admission is checked
before allocating the blueprint. Unknown capacity, unsupported breakout assignment,
mode features and pins cannot be relaxed to finish. PoE total load must fit declared
endpoint maxima; per-partition power is conservatively bounded by total grown load
and that partition's port maxima. No external solver or randomness is used; selection enforces a cooperative deadline.

Test suites:

```sh
cargo test -p labyrinth-engine --test selection
cargo test -p labyrinth-engine --test blueprints
cargo test -p labyrinth-engine --test intent
cargo test --workspace --locked
```

Selection tests cover lexicographic priorities, checked growth, soft spare-port targets,
missing/insufficient throughput and power, port incompatibility, pins, connection
compatibility, stable ordering, bounded search and an exhaustive small port-matching
reference plus exhaustive fixed-assignment ranking reference. Blueprint tests cover
role counts, density overrides, repair exhaustion, stable identities and the explicit
incremental-context limitation. Complete routing/security/resilience and package tests are described in
[core acceptance](../testing/core-acceptance.md). Hosted-worker enforcement remains separate.


## Historical zone distribution refinement — 2026-11-12

`blueprint_rules_v2` supersedes the original direct access/firewall link layout above.
The user chose zone distribution switches to aggregate a shared per-zone VLAN across
access partitions. Each zone now has one distribution switch at Tier0/1, two at
Tier2/3. Access connects to all distribution peers; each firewall connects to its
corresponding distribution peer. Each firewall still connects to all site cores.

Tier1 uses two parallel L2 cables and requires LACP on access, distribution and
firewalls. Tier2/3 dual-homed access and distribution require explicit
`rapid_spanning_tree` mode capability; two distribution-peer links reserve a future
LACP bundle. A dedicated physical firewall HA-sync link is reserved per pair and
classified separately from data transit. No multi-chassis LAG is assumed. Peers use
the full zone peak as a conservative per-device/link/synchronization capacity bound.

`layer2_domains` records each site's zone links and loop-prevention requirement;
`firewall_ha` records paired devices and their reserved control cable. They are
requirements, not active/blocked spanning-tree states, completed LACP bundles or HA
proofs. The current addressing stage does not assign ordinary transit IPs to these
L2/control links. Routed links retain ECMP requirements. Preflight device admission
includes the added distribution roles; exact assignment ranking is unchanged.

Only the synthetic test catalog was expanded to support these capabilities. The
pinned foundation reference bundle and its identity are preserved. Future production
catalogs must declare capabilities rather than inheriting them from a device role.

## Redundant HA control refinement — 2026-11-12

`blueprint_rules_v3` supersedes v2's single synchronization cable with two explicit
control cables per firewall pair. Both consume real selected ports and preserve the
conservative full-zone synchronization capacity bound. L2/domain classifications,
strict scoring and the three-pass repair policy are unchanged. See
[logical-v1](logical-v1.md) for active/standby ownership and the later canonical stage.
