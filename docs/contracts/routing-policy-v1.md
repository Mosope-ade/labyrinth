# Explicit WAN and stateless policy contracts — V1

Implemented contracts; end-to-end acceptance evidence is recorded in [core acceptance](../testing/core-acceptance.md).

## Confirmed WAN declarations

The core accepts explicit `WanService` objects with `site`,
`provider_failure_domain`, `committed_bandwidth_mbps`, and `connected_sites`.
Multi-site input missing these attachments returns clarification. Bandwidth must
be positive and explicit. Site references, distinct peer lists, duplicate site/provider
attachments, and reciprocal service connectivity are checked. The first adapter
requires reciprocal declarations on the same provider domain for a bidirectional
transport. Provider declarations contradicting a site's declared domains reject.
Matching ISP names without explicit peer connectivity never establish a route.

AI may propose complete service attachments from the requested sites and tier while
clarifying requirements. The user must confirm/edit those objects before generation.
The Rust core has no AI dependency and never synthesizes missing confirmed services
or their capacities. A single-site intent may omit WAN services when no WAN is needed.

The hub-spoke adapter selects logical service adjacencies involving a declared hub;
it does not fabricate branch-to-branch shortcuts or carrier cables. Logical WAN
connections reference real selected provider handoff interfaces but remain distinct
from physical internal cables. Multiple handoffs/adjacencies share each attachment's
committed directional budget; duplicating handoffs cannot multiply purchased capacity.
Verification of transport is conditional on those explicit service declarations, not
proof of the provider's internal physical paths or actual service delivery.

## Confirmed security semantics

Policies are directional, stateless, order-independent and default deny. A matching
deny overrides every matching allow. No implicit return permit, connection tracking,
NAT transformation or user rule ordering exists. A positive zone-pair traffic demand
must have at least one permitted service after denies are applied, or generation
rejects it. Capacity is reserved for its declared aggregate demand; a demand with no
service match cannot be disguised as an all-deny valid design.

TCP/UDP destination ranges are inclusive 1–65535; an empty list means all ports.
`any` matches all IP protocol numbers, including protocols outside TCP/UDP/ICMP.
Other IP protocols have no destination-port field in this bounded model. The policy
engine partitions TCP/UDP at rule boundaries and checks one representative from
every equivalence class, plus every other protocol number. This is exhaustive over
the modeled semantics, not arbitrary port sampling.

Rules are installed on each source gateway's logical ingress and every destination
firewall data ingress, including standby peers. Repeated policy IDs at distinct
bindings are permitted only with identical definitions; repeated exact bindings or
contradictory definitions reject. Synthesis and verification must not equate a
physical route with an allowed service.

## Session traffic extension — implementation in progress

The user chose to add per-session and splittable traffic inputs now. Each demand
therefore explicitly chooses `single_path` or `sessions`. A session profile declares
1–64 cohorts, each with positive `session_peak_mbps` and `simultaneous_sessions`.
The sum of cohort peak × count must equal the directional aggregate `peak_mbps`.
This models simultaneous upper bounds, not average session rates. Products are
validated using u128, so oversized envelopes cannot wrap into feasible small values.
Missing distribution information is capacity-critical clarification, not an inferred
traffic model. Individual sessions remain indivisible across paths.

The user selected controlled session steering with explicit capability checks.
Session demands require `session_steering` in every selected data-plane routing mode,
including standby firewalls and redundant cores/edges. Ordinary ECMP or LACP support
cannot substitute. The resulting session-to-path assignments are engineering
requirements for a session-aware steering implementation, not a claim that ordinary
hashing will happen to balance them. Sessions are indivisible on every transport.

When growth is nonzero, session input must explicitly choose `rates` or `concurrency`.
Only that dimension grows, using checked integer ceiling. Aggregate growth targets
and actual session-granularity reservations are separate and visible in routing.json;
the latter may exceed ceil(aggregate × growth) because session units are indivisible.
This is integer granularity, not applying growth twice. The original cohorts remain
ordered identities; unrelated collections normalize by stable IDs for reproducibility.

Path planning is deterministic bounded search over simple paths. It packs compressed
cohort counts rather than materializing every session, and independently verifies
session conservation, path endpoints, live adjacencies, device steering capability,
per-port/mode/shared-group loads and committed WAN attachment budgets. State/path/
queue/depth/time bounds fail explicitly without partial allocations or packages.
A truncated path set is not a global infeasibility proof. Hardware ranking considers
complete-assignment hard verification before accepting a scoring winner; cheaper
capacity-invalid assignments cannot prune a valid higher-tier winner.
