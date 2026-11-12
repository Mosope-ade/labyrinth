# ADR-0001: Configurable addressing and an intermediate topology stage

## Status
Accepted

## Date
2026-11-12

## Context

Selection reserves catalog ports but does not define a functioning broadcast domain,
HA ownership or routing. The first scenario needs concrete connections and IPv4/VLANs,
while future topologies must be able to choose different subnet/gateway conventions.
The user confirmed defaults with the explicit condition that they remain configurable,
and chose zone distribution switches for shared-VLAN aggregation.

## Decision

Keep generic address/VLAN allocators independent of blueprint/device assumptions.
Pass an explicit AddressingPolicy to the first blueprint adapter and persist its value
in AllocatedTopology. Use interval allocation and compressed host ranges, not per-host
iteration. Validate/reserve pins first; reject conflicts without relocation.

Make AllocatedTopology a private-construction intermediate stage rather than creating
an apparently complete NetworkDraft with fabricated provider IPs or forwarding state.
Bind real selected ports/links and reserve endpoint gateways, transits and loopbacks.
Record L2/control requirements separately. The distribution-layer rule revision is
explicitly blueprint_rules_v2; retain catalog identity in the selected result.

## Alternatives considered

- Universal hardcoded conventions would constrain future blueprints and hide defaults.
- Treating access/firewall cables as routed /31s would contradict a shared zone VLAN.
- Assuming firewall bridging or multi-chassis LAG would require undeclared capabilities.
- Giving each access partition a different subnet was offered; the user selected
  distribution switches instead.
- Treating reserved gateways as implemented HA or emitting a verified package early
  would overstate the assurance of this stage.

## Consequences

Address/port behavior can be tested independently with no AI or infrastructure. The
coordinator still fails closed for full incremental updates without prior context.
Canonical assembly must explicitly implement L2 trees/bundles, HA ownership and
provider connectivity, followed by routing/policy synthesis and five-pass verification.
A physical cable or a catalog protocol flag is never sufficient evidence by itself.
