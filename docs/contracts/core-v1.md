# V1 core engine contract

This is a vendor-neutral engineering library. Campus/branch hub-spoke is its first
implemented blueprint/scenario, not the product boundary. Other named blueprints
reject explicitly until implemented. The public `Blueprint` boundary returns common
role, connection, placement, L2 and HA requirements for independent engine stages.

## Entry points

- `intent::parse_json` / `intent::resolve`: validated readiness or explicit clarification.
- `artifacts::generate`: stateless generation through selection, allocation, synthesis,
  verification and package assembly. An incremental intent cannot use this entry point.
- `BaseRevision::from_verified` / `BaseRevision::load_saved`: validated immutable base
  revision context, anchored to an authoritative revision ID and expected manifest hash.
- `artifacts::generate_incremental`: additions against that base, preserving pins and
  re-verifying the complete proposed design. No fallback to redesign is permitted.

`VerifiedPackage` has private construction and no Deserialize implementation. Its
network, report, options and file views are immutable. Bytes are returned only after
all five passes succeed. Failure returns typed clarification/planning/routing/capacity/
verification/budget/revision errors, not a partly valid package. The library performs
no HTTP, AI, database, object-store, authentication or filesystem operations.

## Deterministic engineering package

All payloads are UTF-8 JSON with stable object ordering and newline termination:

- `requirements.json`: normalized effective requirements; unrelated sets sort by ID,
  while ordered session-cohort identities retain their indices.
- `provenance.json`: provided/default provenance using site/zone identity paths.
- `options.json`: addressing, logical/session-routing, verification and search settings.
- `topology.json`: canonical devices, physical links, declared logical WAN connections,
  domains, addresses, L2 states, gateway ownership, routes, traffic paths and policies.
- `catalog.json`: the exact catalog snapshot used for selection.
- `ipam.json`: allocations, compressed host/gateway ranges, VLANs, transits and loopbacks.
- `routing.json`: destination routes, indivisible session allocations, WAN connections,
  aggregate growth targets, reserved peaks and integer session-granularity slack.
- `firewall.json`: directional requested policies and all enforcement bindings, with
  explicit stateless deny-overrides/default-deny semantics.
- `bom.json`: generic archetype/mode/role quantities, relative cost proxy and reference
  hints. These are not currency, purchase SKUs or verified vendor equivalents.
- `selection.json`: candidate exclusions, chosen score, search states and repair evidence.
- `decisions.json`: blueprint/canonical decisions and explicit engineering options.
- `verification.json`: all five passes, proof counts, counterexamples, failure cases,
  protected sets, exclusions and assumptions.
- `manifest.json`: engine/rule/catalog versions and SHA-256/byte counts for every other
  file. Its own digest identifies the package; it is excluded from its own file table.

No wall-clock timestamps, random IDs, vendor configurations or runnable labs enter
core payloads. Identical normalized inputs/options/catalog produce identical bytes.
Hashes establish integrity and reproducibility, not authorship or authorization.
Each file is bounded to 32 MiB and the complete package to 128 MiB. Fixed names do
not accept traversal, paths or user-selected output filenames.

## Selection and feedback

At most three blueprint repair variants are considered. Complete hardware assignments
are ranked by the agreed lexicographic score only after hardware-dependent baseline
and requested failure-state capacity checks. Invalid cheap profiles cannot establish
a pruning bound or beat valid profiles. Search exhaustion/timeouts publish no partial
winner. Ranking is exact over the fixed enumerated candidates and port plans, not a
claim of globally optimal topology, media, traffic engineering or physical hardware.

The final selected assignment receives the complete independent five-pass verifier.
The ranking predicate intentionally does not repeat profile-independent ACL and routing
proofs for every hardware combination. Those checks are mandatory before a package is
constructed. Post-failure capacities remain a hard ranking constraint.

## Incremental additions

The caller supplies the complete updated intent and every prior endpoint-subnet/core
pin; `BaseRevision::update_mode` supplies the exact expected set. Stale base IDs,
omitted/altered pins, changed catalog hashes or modified saved bytes reject. Core
archetypes **and operating modes** remain fixed, not just their identifiers. Every old
CIDR (including transit/loopback space) and VLAN is preserved by identity. Allocation
capacity conflicts reject rather than renumber. Existing access partitions provide
per-zone lower bounds for the three relative repair passes, so additions never remove
old access identities merely because a fresh redesign could use fewer devices.

V1 incremental operation adds endpoints/sites/zones/demands/services. It does not
remove or move existing sites/zones, shrink endpoint populations, reduce existing WAN
capacity/connectivity, change existing endpoint attachments/site roles/locations, or
change global redundancy/growth/headroom/addressing conventions. Use explicit redesign
for those operations. Other unpinned device assignments, physical port bindings and
forwarding can change; this is not a guarantee of configuration-free deployment.
Both successful and rejected proposals leave the base bytes untouched.

For saved packages, the expected manifest digest must come from trusted tenant-scoped
revision metadata, **never from the same untrusted upload/request as the files**.
The hosting layer must authenticate access and establish that authoritative context.
The pure core cannot establish tenant ownership or human confirmation. A digest
self-asserted by an attacker is not an authority. Unsupported saved engine/rule/schema
versions require an explicit migration/reverification or redesign.

## Scope and supported bounds

See [routing/policy](routing-policy-v1.md), [logical](logical-v1.md),
[addressing](addressing-v1.md) and [verification](verification-v1.md) contracts.
Current admission is 128 selected devices, 4096 physical connection requirements,
256 physical interfaces/device, 64 port classes/device and bounded catalog/search
sizes. Session jobs admit 512 cohort work items, 64 default candidate paths per demand
(configurable through 256), 4096 queued partial paths, depth 512, and 100,000 default
search states (through 1,000,000). Sessions use compressed counts, not one object per
session. Candidate-set truncation is explicit search exhaustion, not global infeasibility.

Selection and session search have cooperative 15-second deadlines. Individual
operations are bounded, but a library is not an OS preemption boundary; hosted worker
process supervision must enforce hard CPU/memory/job deadlines. Verification separately
bounds scenario/class work and returns no partial assurance on budget exhaustion.
This does not replace the hosting/security/retention milestones.

Native Rust types generate the stored input/model/options/report/manifest/error schemas.
Schema freshness, strict parsing, byte hashes, positive/negative fixtures, independent
small references, mutation tests and end-to-end scenario tests form the acceptance gate.
