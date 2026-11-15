# ADR-0002: Abstract L2 trees and conservative gateway ownership

## Status
Accepted

## Date
2026-11-12

## Context

Physical distribution-layer cables do not establish forwarding or HA ownership.
A shared per-zone VLAN needs explicit Ethernet loop prevention, and a firewall pair
needs one owner of its virtual gateway. Inventing device-native protocol behavior,
quorum or fencing would overstate the model.

## Decision

Use deterministic abstract steady-state L2 trees over explicit LACP adjacencies.
Keep physical blocked/forwarding cable evidence and unreachable surviving devices.
Expose root preferences without treating the algorithm as a vendor RSTP simulator.

The user approved active/standby ownership with overridable deterministic preference,
no automatic preemption, confirmed-owner failure takeover and fail-closed total
control loss while both peers remain alive. Reserve two control cables per pair in
blueprint_rules_v3 so one cable loss does not immediately trigger that condition.
Keep prior owner history explicit and separate from stateless baseline assembly.

Use explicit canonical gateway, physical attachment, HA-control and unresolved
provider kinds. Structural validation checks their references and address reservations.
Do not invent routes, firewall policy, carrier paths or verification badges.

## Alternatives and consequences

Treating every physical cable as forwarding admits Ethernet loops. Treating LACP as
routed ECMP or assuming firewall bridging hides undeclared capabilities. Treating
silence as confirmed peer failure risks split brain. These are rejected.

Canonical baseline assembly is now testable without AI/services. Actual protocol
convergence, fencing, state synchronization performance, remote reachability and all
five verification passes remain separate work.

The user subsequently approved coordinated transfer when a live owner loses its
modeled local data path and HA control remains healthy. An eligible standby may
then take over; total control loss still fails closed and recovery never automatically
preempts a committed owner. Local path health is not remote/provider reachability
or proof that a real coordination/fencing protocol has executed.
