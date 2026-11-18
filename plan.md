# Labyrinth — Backend-First Implementation Plan

Version: 2.1 · Updated: 2026-11-11
Status: planning baseline; implementation has not started from this document.
Product requirements: [PRD.md](PRD.md)
Author: Adeyinka Adejumo (AdeyLord)

## 1. Direction and agreed boundaries

Labyrinth is a vendor-neutral network topology generator. A deterministic engine
builds the network; an optional AI parser translates user requirements into JSON.
An optional AI assessment reads the completed design and produces a separate report.
AI never selects devices, constructs the graph, allocates addresses, resolves network
constraints, changes verification results, or generates device configurations.

V1 produces an engineering package: topology graph, IPAM/VLAN plan, abstract routing
and firewall policy matrices, BOM of generic archetypes with reference hardware hints,
and verification evidence. Decision logs and resolved assumptions accompany every build.
Vendor-specific CLI configurations, real-device deployment, and runnable virtual labs
are excluded from V1. The catalog and core model do not select vendor SKUs.

CLI and web GUI are separate hosted-service clients. Both call the same backend API;
the V1 CLI does not embed the engine or offer an offline generation mode. The engine,
API, workers, CLI, and GUI have separate responsibilities and build targets. Backend
first; GUI implementation starts only after the backend acceptance gate.

Campus and branch WAN are initial end-to-end validation scenarios, NOT the product's
scope or a special case embedded in the core. The first blueprint supports enterprise
campus/branch hub-and-spoke scenarios. Three-tier campus and spine-leaf are blueprint
extension goals, with availability exposed explicitly rather than assumed from enums.

The old implementation was deleted by the user. The repository currently contains
planning/reference material, not a working engine. This document supersedes the
previous vendor-specific, local-CLI, config-export and lab-export roadmap. Preserve
remaining user material and Git history; do not restore old code implicitly.

## 2. Architecture and deployment boundaries

    CLI executable ─┐
                    ├─ HTTPS → Backend API → durable jobs → generation workers
    Hosted web GUI ─┘              │                          │
                                   ├─ authentication         ├─ pure Rust engine
                                   ├─ database               ├─ pinned YAML catalog
                                   └─ object storage         └─ staged artifacts

The diagram describes responsibility, not a requirement for a separate physical
machine per component. API and workers are independently runnable/scalable services.
The engine is a separately built Rust library invoked by workers, not necessarily an
additional network service. No engine code depends on HTTP, databases, object storage,
authentication, an AI provider, or either interface.

- API: auth/authz, input and revision ownership, parsing/clarification, job admission,
  project metadata, status/events, artifact access, quotas and resource policies.
- Workers: claim durable jobs, resolve immutable inputs, invoke the engine under
  resource limits, persist evidence and stage/publish artifacts, recover attempts.
- Engine: all engineering decisions, selection, topology construction, addressing,
  abstract routing/security realization, verification, deterministic result assembly.
- Database: users/ownership, projects, intents, revisions, jobs/attempts, catalog
  references, artifact manifests, report metadata and separately stored AI assessments.
- Object storage: private engineering packages and larger generated artifacts,
  immutable revision keys, checksums, authorized download and lifecycle cleanup.
- CLI/GUI: input, clarification, job submission, status, inspection and downloads;
  neither duplicates generation logic. Core tests use direct library calls.

Deployment choices still open: hosting provider, database product, authentication
provider/flow, object-storage service, queue implementation, region, retention periods,
and operational SLOs. Axum and a Rust engine are agreed. Select providers after the
workload/security requirements; do not present an unselected vendor as an agreement.
A local development stack must exercise API, workers, database and object storage
without a live AI provider. No production deployment is authorized by this plan.

Suggested source layout, finalized when scaffolding:
- engine/: intent, model, catalog, blueprints, selection, IPAM, routing, policy,
  verification, deterministic package assembly and fixture tests.
- api/: Axum application boundary and versioned contracts.
- worker/: durable execution adapter around engine.
- cli/: hosted API client named laby.
- web/: later GUI client.
- catalog/: YAML archetypes, generated schema, bundle manifest and catalog tests.
- tests/: integration fixtures, failure scenarios and cross-component acceptance.

Use modules before proliferating crates. Public engine types may be shared with API;
the dependency direction must never invert. Algorithm/library names in
algorithm_and_mathematical_models.txt are references, not implementation mandates.
Choose an existing algorithm when it meets the contract; develop and test a custom
algorithm when necessary. Do not adopt ILP, Z3, or parallelism merely because listed.

## 3. Contracts to define before implementation

### 3.1 Intent and readiness

Separate PartialIntent (Option<T> for unresolved quantities/constraints) from resolved,
validated generation input. Track explicit user values, engine defaults, and source
text/provenance separately. Unknown is not zero, false, or an inferred fact.

Required concepts: sites, endpoints/zone, declared simultaneous peak traffic demand,
required allowed and isolated flows, operating/security requirements, redundancy tier,
blueprint preference/override, address constraints and existing-design pins.

Capacity/security-critical gaps trigger clarification. Secondary defaults include
20% growth headroom and IPv4 RFC1918 10.0.0.0/8 base space. Record each default and
allow user override where valid. Default address space is checked against exclusions,
pinned allocations and other known networks; it is not proof of environmental safety.
Missing a second hub location/connectivity for Tier3 requires clarification.

Before coding, freeze demand units/direction/aggregation, flow service matching and
return-traffic semantics, prefix reservations, headroom arithmetic and supported scale.
These are unresolved engineering contracts, not facts already agreed by the user.

### 3.2 Canonical network

Use a vendor-neutral multigraph with stable device/interface/link identities and
explicit sites, failure domains, L2 domains, VLAN membership, L3 interfaces, subnets,
gateways, abstract routing state, firewall enforcement locations and flow policies.
Support parallel links and distinguish LACP bundles from independent routed ECMP
paths. Interface zones differ from a device's administrative classification.

Demand is modeled explicitly, including simultaneous traffic over shared links and
failure-state load. Do not count aggregate bandwidth as guaranteed per-flow bandwidth.
Layered topology and routing/policy graphs may be derived views of the same canonical
network; do not treat a physical topology cycle as a forwarding loop.

### 3.3 Catalog

YAML entries are generic archetypes, never vendor-specific model selections. Include
stable archetype ID, eligible roles, relative cost tier, port groups (count/speed/media,
breakout/shared restrictions), PoE budget, logical protocol/security capabilities,
and named operating-mode profiles. Capacity depends on the selected mode, e.g.
base_throughput_gbps versus ips_vpn_throughput_gbps; do not interpolate unsupported
feature combinations or treat missing capacities as unlimited.

Reference hardware hints are informational examples, separate from constraint solving
and rankings. No vendor preference/affinity scoring. BOM quantities refer to archetypes,
not purchase-ready SKUs or guaranteed prices.

Generate strict JSON Schema from Rust types using schemars; validate YAML against it
in the build/test gate. Add semantic checks for contradictory capacities, invalid port
combinations, units and role/mode requirements. Pin a semantic version plus SHA-256
content digest; catalog_version records both unambiguously. Archive the exact bundle
and retain it for every retained project revision. Never resolve an old build to latest.

## 4. Engine pipeline and selection

Conceptual API:

    generate(resolved_intent, catalog_snapshot, engine_context, prior_revision?)
        -> Result<VerifiedEngineeringPackage, GenerationError>

No ambient network calls, wall-clock-based choices or hidden random inputs. Keep
incomplete intermediate states distinct from exportable verified results.

Pipeline:
1. Validate/normalize resolved intent, feature support, catalog version and update pins.
2. Select explicit blueprint, or apply versioned rules using site count, endpoint
   density and redundancy tier. An explicit override must pass feasibility checks;
   never silently replace it with an unrelated blueprint.
3. Derive logical roles, quantities, connections and engineering demand.
4. Select generic archetypes through the three-step process below.
5. Allocate physical ports/links and finalize logical topology, IPAM/VLANs, abstract
   routing and firewall policies from the selected design.
6. Execute all five verification passes, including failure-state capacity.
7. Assemble a deterministic engineering package and manifest only after required gates
   pass. Workers publish it; failed/inconclusive builds produce diagnostics only.

Selection process:
- Role mapping: blueprint derives required roles and role-specific requirements.
- Hard filtering: check port compatibility/density, operating-mode throughput, PoE,
  required logical features, capacity and security isolation; preserve explicit pins.
- Rank feasible full assignments, not isolated device choices, lexicographically:
  (1) all hard constraints satisfied; (2) peer-role archetype standardization;
  (3) lowest relative cost tier; (4) spare-port headroom target 20% and minimum
  device count. Stable archetype-ID ordering resolves remaining equal scores.

This is strict priority, not a weighted sum. Growth demand and spare-port headroom
are distinct calculations; do not silently double-count 20%. Settled cost aggregation is summed relative tiers per installed device; priority
(4) minimizes spare-port shortfall before device count. See
[selection contract](docs/contracts/selection-v1.md) for the complete scoring formula. Do not call a heuristic result globally optimal without proof.

Bound selection/topology feedback to at most three passes. Deterministic repairs
may upgrade archetype capacity or scale horizontally with parallel switches/uplinks.
A repair invalidates dependent port assignments, addressing, routing and policy;
recompute and reverify those outputs. Pins and isolation cannot be weakened to fit.
Report pass history, bottleneck, rejected candidates and possible resolutions.
Distinguish proven infeasibility from no solution found within the three-pass budget.
A solver timeout is inconclusive, never evidence of infeasibility or success.

## 5. Verification contract

Each pass emits status, assumptions, checked property, component IDs, versioned
method, evidence/counterexample, and scope limits. Top-level statuses are passed,
failed or inconclusive; unsupported/unmodeled checks cannot pass. No verified package
is published unless every required check passes. These prove properties of the
abstract engineering model, not behavior of an actual device deployment.

### Structural

Check intended connectivity (not indiscriminate all-to-all reachability), references,
port allocation/count/speed/media matching, logical bundles and mode capabilities.
Zero uplink oversubscription means enough capacity for declared simultaneous peak
demand with growth, including requested failure states; not sum of endpoint line rates.

### Addressing

No overlapping allocated subnets within the same routing/address realm; no duplicate
host assignments, invalid gateways or insufficient usable host capacity. Parent pool
containment is valid and differs from overlap between sibling allocations. Respect
reserved/excluded ranges, VLAN scope and pinned allocations.

### Routing

Required allowed flows have reachable next hops and loop-free modeled forwarding
paths. Record route choice/failover assumptions and destination-specific checks;
physical cycle detection alone is insufficient. Routing representation remains abstract.

### Security

Formal reachability evaluation over modeled packet classes, routes, ordered firewall
policies and enforcement boundaries: isolated zones must have zero permitted paths.
Provide a violating path/packet class on failure. Allowed flows must also work; denying
everything is not a valid solution. Scope any return-traffic/stateful assumptions.

### Resilience

- Tier0_None: single devices/links allowed; no resilience guarantee. Record baseline
  connectivity and exclusions, rather than claiming survival tests passed.
- Tier1_Link: dual uplinks using modeled LACP/ECMP; required paths survive any relevant
  single-link removal with sufficient capacity. Single devices remain allowed.
- Tier2_Device: Tier1 plus dual-homed access, HA firewall pairs and redundant
  core/distribution where relevant. Protect backbone/required paths among surviving
  devices against single-device loss. Single-homed endpoint attachment is excluded;
  do not claim its endpoint survives removal of its sole access device.
- Tier3_Site: Tier2 plus dual-WAN/multi-ISP and geo-redundant hubs. Require two hub
  locations and independent connectivity; clarify if only one was supplied. Test
  whole-site and ISP/circuit failure domains in addition to link/device removals.

Independence is explicit: two links through the same provider/upstream component do
not prove independent protection. Re-evaluate forwarding, security and demand under
each protected failure. Do not promise restoration of a failed site's own endpoints
unless application/endpoint replication is separately modeled. Steady-state failure
analysis is not a quantified convergence-time guarantee.

## 6. Revisions, reproducibility and outputs

Stateless Redesign computes a new design from requirements without previous-assignment
pins. Incremental Update pins existing IPAM allocations and core-device assignments
while adding endpoints/sites. Recompute all affected derived outputs and verify the
whole result. Incompatible pins produce a conflict with proposed changes; never unpin
silently. Require an expected base revision to reject stale concurrent updates.

Reproducibility inputs include resolved intent/defaults, base revision and pins,
engine/blueprint/rule/verifier versions, exact catalog hash and search configuration.
Stable IDs, iteration order, fixed-point/scaled numeric units, canonical serialization
and tie-breaking determine the design. Timestamps, job IDs and AI report metadata are
separate provenance. Timeout/resource-limit outcomes do not claim reproducible design
success. Never cache across tenant boundaries without explicit isolation design.

Package contents: canonical graph JSON; IPAM and VLAN data; abstract routing matrix;
firewall policy matrix; archetype BOM/reference hints; five-pass verification report;
decision log; resolved intent/defaults; manifest with input/version/content hashes.
Human-readable report and CSV views derive from the same revision. Exact filenames
and additional visual exports are specified before artifact implementation.

Stage objects privately, verify hashes, then transactionally publish the database
manifest and revision as the visibility boundary. Failed/cancelled jobs never expose
a verified partial package or replace the last good revision. Clean orphaned staging
objects after a configured grace period; DB and object storage are not a shared ACID
transaction. Authorized downloads reference published immutable revisions only.

## 7. Hosted API, jobs and clients

Design versioned JSON/OpenAPI contracts before the CLI/GUI. Required operations:
authentication, project/revision management, partial-intent validation/clarification,
asynchronous generation, status/event recovery, reports, artifact download, cancellation,
and separate AI assessment requests. Both interfaces accept structured intent as an
AI-free path. Planned CLI capabilities: login, validate, generate, jobs/status, inspect,
download. Commands are proposals until the CLI contract is frozen; no offline mode.

Axum accepts valid work with 202 + job ID. Persist job inputs, immutable catalog
reference and idempotency key before execution. Jobs: queued → running → succeeded,
failed, inconclusive or cancelled; record worker leases and attempts. Retry transient
execution/storage failures with bounded attempts; do not retry deterministic input
conflicts as if transient. Restart/reconnect must recover authoritative terminal state
without relying on an always-connected WebSocket.

Enforce tenant ownership on projects, revisions, intent sessions, jobs, events,
assessments, object keys and signed downloads. Add per-user concurrency, topology/input
size and solver/worker CPU-memory budgets. Use 15 seconds as the initial maximum
wall-clock safeguard per solver invocation, configurable downward by service policy;
also bound total job duration and deterministic search effort. Select exact total
limits after representative fixtures and before admitting production workloads.
Run non-cooperative solvers in killable execution boundaries; an async timeout alone
must not leave computation consuming resources.

Retention is explicit for published revisions, failed diagnostics, raw AI messages,
staging objects, job metadata and deletion/backup windows. The accepted primary-data
policy is recorded in docs/contracts/retention-v1.md. Optional AI message retention
and live backup expiry/restore acceptance remain release gates. Log identifiers/statuses, not sensitive
intent payloads or topology contents. Redact assessment context and define external
AI data sharing before enabling it. No device credentials are needed for V1 outputs.

## 8. Optional AI boundary

AI only parses user requirements into the versioned partial-intent JSON and may assess
a completed immutable topology afterward. Engine checks readiness/semantics and supplies
critical missing fields for the interface to clarify. Validate every AI response;
structured output support never substitutes for validation. Do not let AI invent
capacity values or select a catalog archetype as an authoritative engineering choice.

Assessment is optional, read-only, version-bound and separate from deterministic
verification/decision evidence. It cannot trigger edits, change verification status
or gate exports. Structured submission, running jobs and downloads work without an
AI key or during provider outage. Changes enter as validated user intent against an
expected base revision. Provider/model, retry/fallback policy and context retention
are open decisions to settle before AI implementation.

## 9. Milestones and execution gates

Milestone status is recorded below. Each milestone is split into focused tasks;
PRD requirement IDs provide acceptance traceability. No frontend task begins early.

### M0 — Freeze contracts and scaffold separate components

- [x] Record remaining open decisions from PRD section 12.
- [x] Define partial/resolved intent, canonical model, errors and report schemas.
- [x] Establish Rust workspace, fixtures, schema/catalog checks and CI commands.
Gate: no stale vendor-config/local-CLI assumptions; contracts distinguish clarification,
invalid input, unsupported request, infeasibility, search exhaustion and timeout.

### M1 — Catalog and deterministic selection

- [x] Build archetype YAML schema and semantic validation; pin version/hash bundles.
- [x] Implement role mapping, hard filtering and strict full-assignment ranking.
- [x] Implement the three-pass upgrade/horizontal-scaling loop and decision evidence.
Gate: mode-dependent sizing, port/PoE limits, tie-breaking, ranking, repairs and pinned
constraints have positive/negative tests. Reference hints cannot change feasibility.

### M2 — First complete engine slice

- [x] Implement generic blueprint interface; first campus/branch hub-spoke scenario.
- [x] Resolve port/L2/L3 connections, IPAM/VLANs, abstract routes and firewall policies.
- [x] Add all five deterministic verification passes and package assembly.
Gate: 1 hub + N branches scenario produces the entire neutral package with no external
AI/HTTP/storage dependency in engine tests. This is scenario coverage, not product scope.

### M3 — Resilience and update-mode completion

- [x] Cover Tier0–Tier3 using explicit protected sets and failure domains.
- [x] Add peak-demand/growth and post-failure capacity checks.
- [x] Implement Stateless Redesign and pinned Incremental Update.
Gate: Tier3 two-hub fixture passes site/ISP failures; single-hub Tier3 requests clarify;
pin conflicts block; stale revisions reject; identical inputs reproduce outputs.

### M4 — Hosted asynchronous backend

- [x] Select infrastructure providers: Cloudflare Pages, Render API/workers, Supabase
  Postgres/Auth/Storage/Queues and GitHub Releases; see ADR 0003.
- [ ] Finalize regions, service sizing, auth flows and operational limits before deployment.
- [ ] Build Axum contracts, tenant isolation, revisions, workers and staged publication.
- [x] Add idempotency, leases/recovery, cancellation and enforceable resource limits.
- [ ] Define admin RBAC/audit boundaries, operational/AI telemetry, usage reporting,
  catalog draft-validation/promotion APIs and safe user/job administration (FR-20–FR-22).
Gate: AI-free structured submission → async job → persisted verified revision → download;
worker/API restart recovery; cross-tenant denial; no partial published package.

### M5 — CLI and optional AI backend

- [ ] Ship a separate hosted CLI over the published API contracts.
- [ ] Add JSON intent parsing/clarification and separate read-only assessment.
- [x] Implement structured diagnostics, policy-bound failed payload erasure and
  owner-confirmed project deletion/recovery with bounded purge.
- [ ] Configure and verify live backup expiry/restore and optional AI-message retention.
Gate: CLI covers the full hosted engineering workflow; provider failure cannot break
structured generation; assessment cannot mutate a design; deletion/cleanup verified.

### M6 — Backend V1 acceptance

- [ ] Run scenario corpus and adversarial cases across all PRD requirements.
- [ ] Publish support matrix, assurance limits, API/CLI docs and deployment/runbooks.
- [ ] Benchmark limits, test restore/recovery and complete isolation/security review.
Gate: every backend-applicable V1 requirement has test evidence; backend release blockers are closed;
no vendor configs, procurement guarantee, emulation or real-world enforcement claim.

### M7 — GUI, admin dashboard and later capabilities

After backend acceptance, build the separately hosted GUI against the same API, with
structured input, clarification, async progress, inspection, reports and downloads.
Browser usability/accessibility testing is required. Additional blueprints (including
three-tier campus/spine-leaf), address families and modeling features enter through
versioned capability slices with their own support/verification evidence. Local/offline
CLI execution, desktop packaging, device config adapters and labs require new scope
agreements; they are not inherited requirements from the deleted implementation.

## 10. Frontend and admin delivery contract

Implement after backend gates, using React + TypeScript, React Flow, Tailwind CSS,
shadcn/ui and @monaco-editor/react. No GUI implementation begins in the current
backend phase. Admin API/security/telemetry contracts are backend deliverables.

- Three-pane workspace: bounded resizable/collapsible requirements/tree panel, flexible
  canvas and context inspector. Constrain non-canvas/admin content width; keep primary
  ultrawide expansion in the canvas. Support touch and accessible keyboard navigation.
- Separate visual layout from engineering state. Save positions/grouping/viewport
  against stable revision/entity IDs. Evaluate layout engines on grouped and parallel
  link fixtures; background expensive layout and reject stale results. Preserve manual
  positions and viewport during job updates. Do not require a specific library yet.
- V1 canvas is layout/inspection only. No adding/deleting devices/links or altering
  pinned constraints directly; engineering changes use requirements and regeneration.
  Failure overlays show actual backend evidence, not client-authored verification.
- Define original vendor-neutral device icons, grouping, HA/bundle treatment, zoom-level
  detail and non-color statuses. Selecting evidence highlights affected components.
- Monaco edits JSON/YAML intent and catalog drafts, inspects immutable artifacts and
  diffs. Generated artifacts/published catalogs stay read-only. Lazy-load heavy views.
- Below 768 CSS px replace workspace AND admin UI with exactly: "Designed for desktop,
  laptop, and tablet workspaces. Please open on a larger screen". Preserve drafts on
  resize. Tablet widths use overlay drawers; larger screens progressively expose
  capped panes. No user-agent gating or page-level canvas overflow.
- Admin modules: operations/solver outcomes, AI usage/estimated costs, catalog inspector
  and version lifecycle, user/quota usage, suspension, job cancellation/recovery and
  append-only privileged-action auditing. Redact content and preserve tenant privacy.
- Catalog publishing is immutable, validated and version/hash-addressed; promotion or
  rollback changes the default for future builds only. Preview drafts against regressions.
  Job recovery preserves attempts/leases and cannot duplicate a published revision.
- Enforce backend User/Admin access, role revocation/suspension and audit capture for
  sensitive mutations. UI hiding is not authorization. Do not expose private project
  contents or raw prompts simply because someone has an admin role.

M7 task sequence: tokens/icon primitives → shell/viewport guard → layout/inspection →
requirements/Monaco → reports/jobs/downloads → admin read views → admin controlled
mutations → keyboard/touch/accessibility and responsive/runtime acceptance.
Gate: FR-17–FR-22 evidence at phone fallback, tablet, laptop and ultrawide widths;
layout does not change engineering digest; no lost draft, stale-result overwrite,
unauthorized admin action or unverifiable failure overlay. Full V1 acceptance includes
both backend and client gates.

## 11. Verification and working rules

Planned commands, available only after scaffolding (not claims of current executability):

    cargo fmt --all -- --check
    cargo check --workspace
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

CI additionally runs explicit catalog/schema checks and hosted integration tests
against disposable database/object-storage services. Keep unit tests hermetic; engine
fixtures need no AI key. Test boundaries, counterexamples, properties and repeated
builds rather than implementation constants. Use independent small reference models
or exhaustive fixtures to cross-check optimization/verification algorithms.

Required corpus: missing critical inputs; default provenance; operating-mode throughput;
PoE/port incompatibility; feasible and infeasible demands; three-pass exhaustion;
strict standardization/cost/headroom ordering; allowed and isolated traffic; CIDR
exhaustion; four failure tiers; shared-provider failures; load after failover; pins;
reproducibility; catalog tampering; timeout; worker kill/restart; cancelled publication;
cross-tenant access; storage failure/cleanup; AI outage and assessment immutability.

Do not restore/rewrite code before the contracts are settled. Do not weaken constraints,
skip failing checks, invent successful evidence or silently broaden proof claims.
Algorithms are chosen by correctness and measured need; unfamiliar algorithms require
reference fixtures and complexity analysis. Keep changes reviewable and preserve user
work. This plan authorizes no production publishing or direct device configuration.

### Engineering standards and readiness

Scalability, security, reliability and maintainability apply from the first slice.
Use the explicit PRD section 6 baseline as the quality contract: bounded work/queues,
independent scaling, strict TypeScript/typed Rust boundaries, documented APIs, least
privilege, threat modeling, server-side ownership/RBAC, validated inputs, protected
secrets, private storage and dependency hygiene. Keep code cohesive and readable;
new abstractions must earn their complexity. Do not equate extra infrastructure with
scalability or a passing test suite with comprehensive security.

Each slice requires appropriate tests, formatting/type/lint checks and code review;
CI adds dependency/secret scanning and applicable integration checks. Add load,
resource-bound, restart, cancellation, storage-publication, backup/restore and admin
negative tests at their owning milestones. Report scope and limitations of evidence.
No check suppression, silent failure, default credential or fabricated readiness claim.

We can start M0 coding: workspace scaffolding, fixture/test harnesses, typed boundary
prototypes and CI. Resolve outstanding engine contracts before committing dependent
behavior; resolve infrastructure choices before provisioning/integration, and visual
choices before GUI implementation. Existing open decisions do not block independent
foundation work. This is readiness to begin, not a declaration that every release
or deployment decision is settled.

## Implemented foundation checkpoint — 2026-11-11

Intent/model/catalog contracts and fixtures are implemented; see
[engine-v1 contracts](docs/contracts/engine-v1.md). Directional integer-Mbps demands
and explicit stateless protocol/port policies were confirmed by the user. Readiness,
structural draft integrity and catalog validity are separate from generation/verification.
Generated schemas, catalog digest/snapshot checks and Rust CI gates are in place.
At this checkpoint, selection, allocation and five-pass verification were not yet
implemented. Later checkpoints below record their subsequent progress.

## Selection/blueprint checkpoint — 2026-11-11

The user confirmed: growth applies once (ceil), spare ports are a separate soft target,
peer standardization precedes summed relative-tier cost, then headroom shortfall,
device count and stable IDs. Endpoint attachment defaults to 1 Gbps copper and is
overridable/provenanced. See [selection-v1](docs/contracts/selection-v1.md).

Fixed-role filtering, physical port matching, bounded exact assignment ranking,
hub-spoke role planning and three-pass access expansion are implemented and tested.
Unsupported blueprints reject; full incremental blueprint planning awaits prior-revision
context and rejects safely. Selected plans do not constitute generated/verified networks.
At this checkpoint, IPAM allocation, route/policy synthesis and five-pass verification
remained unimplemented. The addressing checkpoint below supersedes IPAM status.


## Physical binding/addressing checkpoint — 2026-11-12

`pipeline::allocate_topology` now binds selected catalog ports to physical links and
endpoint/provider reservations, and allocates endpoint/transit/loopback IPv4 space
plus VLANs. See [addressing-v1](docs/contracts/addressing-v1.md). Growth is not repeated.
Low-level allocators enforce exclusions, capacity, deterministic order, exact-prefix
requests, optional pins, VLAN uniqueness scopes and bounded admission.

The user confirmed configurable defaults: one subnet/VLAN per site zone; one gateway
reservation for Tier0/1 and three for Tier2/3; /31 routed links, optional /32 loopbacks;
VLANs starting at 10 with reuse across sites. These are explicit policy inputs stored
with the result, not universal topology rules.

This is an intermediate `AllocatedTopology`, not a complete canonical network or
verified package. M2 remains open: L2 aggregation/HA ownership, routing, policy and all
five verification passes must be implemented. Distribution-layer cables do not
establish a shared VLAN without explicit L2 forwarding realization. Prior-revision context
and full incremental-update support also remain open.


The user also selected zone distribution switches for shared-VLAN aggregation.
Blueprint rules v2 now add the distribution layer, explicit LACP/RSTP capabilities,
L2-domain link requirements and dedicated firewall HA-control links. Capability
selection and port allocation are tested; actual L2/HA forwarding remains open.


## L2/HA and canonical assembly checkpoint — 2026-11-12

Abstract deterministic L2 trees (LACP adjacency collapse, forwarding/blocked evidence,
confirmed-failure recomputation), active/standby owner evaluation and baseline canonical
assembly are implemented. The user approved deterministic overridable owner preference,
no automatic preemption, confirmed-owner-device failure takeover, and fail-closed total
HA-control loss while both peers remain alive. Blueprint rules v3 reserve two HA-control
cables per pair. See [logical-v1](docs/contracts/logical-v1.md) and ADR-0002.

`pipeline::assemble_network` produces structurally validated canonical entities with
explicit gateway member/VIP ownership, L2 states, LACP/ECMP groups and unresolved
provider handoffs. Route/policy collections are intentionally empty at this stage;
M2's synthesis, all five verification passes and final package assembly remain open.
Tier0–Tier3 shape coverage is not resilience sign-off. Full incremental updates and
protocol convergence/fencing behavior remain outside this implemented contract.


### Confirmed HA path-health behavior — 2026-11-12

The user approved coordinated standby takeover for a live owner that loses its
modeled local L2 or all routed-transit attachments, provided HA synchronization is
healthy and the standby is eligible. Total control loss while both peers live still
fails closed; an ineligible pair remains unavailable; recovery never preempts the
committed owner. Tests cover distribution failure, attachment loss, all owner-side
core-link loss, simultaneous synchronization loss and recovery. Remote/provider
health and five-pass resilience assurance remain separate work.

## Core-completion work in progress — 2026-11-12

The user confirmed explicit WAN-service attachments inside the core, with AI-proposed
values confined to clarification and requiring user confirmation. The core must never
infer transport connectivity from ISP strings. Deny-overrides, default deny and rejection
of positive demands without permitted services are also confirmed. See
[routing-policy-v1](docs/contracts/routing-policy-v1.md).

WAN intent checks, service classification and destination-tree routing tests have passed.
Five-pass evidence, failure-state resource accounting, package assembly and pinned update
completion remain active work, not accepted milestones. Traffic/session granularity needs
a user decision before final capacity and resilience semantics can be frozen.


### Session traffic extension — 2026-11-12

The user selected per-session/splittable traffic inputs now. Explicit traffic profiles
and simultaneous session cohorts are implemented/validated; the sum of cohort upper
bounds must equal declared aggregate peak. Missing profiles clarify, and routing
rejects session profiles until the multipath implementation is complete. Controlled
session steering versus worst-case ECMP/LACP hash-skew sizing needs a decision before
that behavior is implemented. Core completion remains unachieved.


### Controlled sessions, verified packages and pinned-context progress — 2026-11-12

The user approved controlled session steering with mode-capability checks. Session
cohort packing, explicit rates/concurrency growth choices, independent conservation
checks and shared service budgets are implemented and tested against an exhaustive
small reference. A five-pass-verified, deterministic 13-file neutral package is now
assembled with BOM, catalog snapshot, provenance and SHA-256 manifests.

Incremental endpoint expansion preserves all previous CIDRs/VLANs and exact core
archetype/mode assignments using an authoritative base revision and manifest digest.
Stale revisions, incomplete/altered pins, catalog mismatches, tampered saved payloads
and insufficient pinned capacity reject. New-site expansion and broad scenario/scale
acceptance remain under test; do not mark core completion from this checkpoint alone.

## Core acceptance checkpoint — 2026-11-12

M0–M3 gates passed. See [requirement evidence, commands and assurance limits](docs/testing/core-acceptance.md).
The workspace passed 124 tests including five compile-fail checks; the complete
AI-free example produces a verified 13-file package manifest. The 126-device,
496-failure-case regression passes. M4+ remains unfinished and is not implied by
core acceptance. Earlier dated checkpoints record intermediate states; current
contracts and the acceptance checkpoint supersede their pending-work notes.

## Hosted infrastructure checkpoint — 2026-11-15

Provider decisions are recorded in [ADR 0003](docs/decisions/0003-hosted-infrastructure.md).
Next: tenant/project/revision schema and transactional job admission, lease/fencing,
staged publication and recovery contracts. No services have been provisioned.

The initial [hosted job lifecycle contract](docs/contracts/hosted-jobs-v1.md) defines
tenant ownership, transactional admission, attempt fencing, cancellation, staged
publication and recovery acceptance cases. Runtime implementation remains pending.

## Database/admission checkpoint — 2026-11-15

- [x] Initial SQLx migrations: users, tenant memberships/quotas, catalogs, projects,
  jobs, attempts, immutable revisions/artifacts and append-only admin audit records.
- [x] Bounded prepared requests and atomic authorization/idempotency/head/catalog/
  quota checks with job insertion and pgmq dispatch.
- [x] Real PostgreSQL/pgmq integration cases and a dedicated CI gate.

See [ADR 0004](docs/decisions/0004-transactional-admission.md) and
[database acceptance](docs/testing/database-admission.md). No hosted deployment,
HTTP listener, authentication/provisioning flow or worker execution is implied.
Next: authenticated Axum admission/status boundaries, then worker lease/fencing
and supervised execution with staged publication. Operational limits and auth
flows must be settled before the corresponding runtime behavior.

## Authenticated HTTP checkpoint — 2026-11-15

- [x] Supabase ES256/RS256 user-token verifier with bounded, rotating JWKS cache.
- [x] Authenticated Axum job admission and scoped authoritative status polling.
- [x] Private catalog/status read functions with current membership/suspension checks.
- [x] Explicit bounded server configuration, TLS database connections, restricted
  runtime role, payload/concurrency/deadline guards and redacted request telemetry.
- [x] Signed-token/live HTTP/PostgreSQL tests, invalid-input and failure tests.

See [HTTP contract](docs/contracts/http-jobs-v1.md) and
[ADR 0005](docs/decisions/0005-authenticated-job-http.md). Login/provisioning,
scripted CLI credentials, frontend CORS origins and hosted deployment remain pending.
Next runtime slice: worker claim leases/fencing, supervised core execution and
staged artifact publication. Numeric job CPU/memory/time/retry limits must be settled
before worker execution; existing HTTP examples are not production workload SLOs.

## Fenced worker checkpoint — 2026-11-16

- [x] User-approved configurable initial worker policy: one job/process; lease 60s,
  heartbeat 15s, total attempt 120s, child CPU 60s/address space 1 GiB, serialized
  input 2 MiB/package output 32 MiB, three attempts, transient backoff 10s then 60s.
- [x] Separate restricted worker role, immutable context reads, atomic claims,
  queue/lease renewal, fencing, redelivery recovery and durable retry exhaustion.
- [x] Killable supervised engine process, no inherited environment credentials,
  deadlines and resource limits; saved requirement provenance survives restoration.
- [x] Private attempt-specific non-overwriting uploads, byte read-back checks and
  atomic fence/head-checked revision/artifact publication with queue acknowledgment.
- [x] Real PostgreSQL/pgmq, real engine, storage fault injection and regression tests.

See [ADR 0006](docs/decisions/0006-fenced-worker-publication.md),
[worker contract](docs/contracts/worker-v1.md) and
[acceptance](docs/testing/worker-publication.md).
Next: authorized revision/artifact downloads, user cancellation/diagnostics,
orphan retention/cleanup and deployment/provisioning/restore testing. M4 remains
open; numeric retention and deployment sizing are still release decisions.

## Cancellation/download checkpoint — 2026-11-16

- [x] Authenticated, tenant-scoped, idempotent cancellation with current write-role
  checks; queued jobs cancel immediately and running jobs lose publication rights.
- [x] Published-artifact proxy downloads with current membership checks, authoritative
  size/hash checks, post-fetch reauthorization and bounded response-body capacity.
- [x] Narrow database functions and signed-request/storage HTTP regression tests.

Orphan cleanup implementation awaits the explicit safety-window decision. Published
revision retention remains separate. Live deployment and storage provisioning are
still pending; no production credentials or services were used by these tests.

## Orphan cleanup checkpoint — 2026-11-16

- [x] User confirmed 24-hour staging safety window; explicit worker setting 86400.
- [x] Terminal timestamps, bounded leased cleanup claims, reference rechecks and
  database guards that prevent failed attempts gaining publication references.
- [x] Exact-inventory private storage deletion, partial-failure/crash recovery and
  repeat sweeps for late stale-worker uploads.
- [x] Real PostgreSQL/engine integration checks preserve all published bytes;
  HTTP deletion adapter tests reject unsafe inventories and exercise retry success.

See [cleanup contract](docs/contracts/orphan-cleanup-v1.md) and
[acceptance](docs/testing/orphan-cleanup.md). This supersedes the prior pending
orphan-window decision. Published revision retention, project/data deletion,
restore drills, detailed diagnostics and live deployment acceptance remain open.

## Project/revision discovery checkpoint — 2026-11-16

- [x] Bounded tenant project listing and project-scoped published revision listing.
- [x] Published revision detail with artifact names/hashes/sizes; no object-key or
  requirement-payload disclosure.
- [x] Explicit UUID cursor semantics, current membership checks and restricted
  database read functions with signed-request/PostgreSQL acceptance tests.

See [contract](docs/contracts/project-discovery-v1.md) and
[acceptance](docs/testing/project-discovery.md). Project/tenant creation and other
management mutations, authentication onboarding, diagnostics, admin APIs and hosted
deployment/restore acceptance remain separate unfinished backend work.

## Project creation checkpoint — 2026-11-16

- [x] Tenant-scoped owner/editor project creation and member-readable project detail.
- [x] Immutable actor-scoped idempotency records, serialized quota admission and
  strict bounded names/body validation.
- [x] Explicit project-cap configuration; no inferred production quota or automatic
  tenant/member provisioning. Concurrent signed-request/PostgreSQL tests.

See [contract](docs/contracts/project-creation-v1.md) and
[acceptance](docs/testing/project-creation.md). Authentication onboarding is paused
for the user's login-method, provisioning and quota decisions. Existing bearer/JWKS
verification remains in place. No frontend/CLI login UI or hosted auth configuration
has been created by this checkpoint.

## Explicit personal onboarding checkpoint — 2026-11-16

- [x] User selected Supabase email magic links with PKCE; GUI/CLI own authentication
  sessions, code exchange, refresh and logout; API remains an access-token resource server.
- [x] User selected explicit personal-workspace creation (one per authenticated
  subject), initial two active jobs and 50 persisted projects; configured explicitly.
- [x] Side-effect-free identity inspection, concurrent idempotent onboarding, immutable
  personal mapping, current permission/suspension checks and no administrator grants.
- [x] Replay preserves quotas and cannot restore revoked membership or demoted roles.
- [x] Signed-token/PostgreSQL acceptance, project-quota checks, provider setup runbook.

See [ADR 0007](docs/decisions/0007-personal-workspace-onboarding.md),
[contract](docs/contracts/auth-onboarding-v1.md) and
[acceptance](docs/testing/auth-onboarding.md). This supersedes the prior pending
login-method/provisioning/initial-quota decisions. Hosted provider configuration,
actual email/PKCE/refresh/logout acceptance and GUI/CLI auth clients remain pending.

## Attempt diagnostics checkpoint — 2026-11-16

- [x] Tenant-scoped bounded attempt history with fixed redacted messages/outcomes.
- [x] Fenced atomic worker failure reporting and durable retry scheduling metadata.
- [x] Resource/search exhaustion remains inconclusive across the child boundary;
  generic failure is never asserted to prove network infeasibility.
- [x] Signed-request/PostgreSQL, retry-fencing, real subprocess and storage-fault tests.

See [contract](docs/contracts/job-diagnostics-v1.md) and
[acceptance](docs/testing/job-diagnostics.md). Full core counterexample transport,
overall metadata retention, admin APIs and hosted deployment/restore acceptance
remain open. No raw diagnostics/exception-text artifact store has been introduced.

## AI-free client workflow checkpoint — 2026-11-16

- [x] Current admitted catalog metadata discovery and hash-validated bundle reads.
- [x] Tenant-authorized structured-intent validation shares admission's readiness,
  blueprint and options checks; no AI, generation or persistent draft is required.
- [x] Bounded project job listing for reconnect/status recovery.

See [contract](docs/contracts/client-workflow-v1.md) and
[acceptance](docs/testing/client-workflow.md). Admin mutation boundaries and deployment
packaging/acceptance remain separate work; no catalog default is silently selected.

## Admin governance checkpoint — 2026-11-16

- [x] User-approved operator-managed admin registry with per-operation revocation/
  suspension checks; no JWT metadata escalation or automatic private-content access.
- [x] Operational overview, bounded user/tenant/audit metadata, confirmed suspension
  and state-only job intervention/cancellation without rewriting terminal history.
- [x] Strict draft validation, reviewed hash promotion, immutable semantic-version
  mapping and admission compare-and-set with archived replay preservation.
- [x] Atomic append-only audits, operator role-change attribution and a separate
  maintenance-only 365-day minimum bounded retention tool.

See [ADR 0008](docs/decisions/0008-admin-catalog-and-audit-governance.md),
[contract](docs/contracts/admin-v1.md), [runbook](docs/runbooks/admin-operations.md)
and [acceptance](docs/testing/admin-operations.md). AI-specific telemetry, full
counterexample transport, actual hosted provisioning and deployment/restore/load
acceptance remain unfinished. No admin GUI has been built.

## Deployment preparation checkpoint — 2026-11-16

- [x] Pinned non-root backend container definition, separate API/worker commands and
  explicit operator tools; no auto-migration or owner credentials in runtime services.
- [x] Offline real-engine/container smoke workflow, strict provider schema validation
  with independently hash-locked tooling and credential-safe build-context exclusions.
- [x] Bounded readiness distinct from liveness, including role/schema and refreshable
  signing-key dependencies; provider errors remain redacted.
- [x] Reviewable staging proposal and role-separated provisioning/rollback runbook.

See [deployment preparation](deploy/README.md),
[runbook](docs/runbooks/staging-deployment.md) and
[acceptance](docs/testing/deployment-preparation.md). Region/budget
approval, provider credentials/provisioning and live load/restore/Auth/Storage checks
must be confirmed separately. Local native smoke is not a container deployment claim.

## Admission contract export checkpoint — 2026-11-16

- [x] OpenAPI 3.1 request subset for onboarding, project creation and job admission,
  with engine-derived intent/options schemas, strict authority boundaries and
  export-freshness/reference validation tests.
- [x] Typed success responses and discovery/admin/download OpenAPI coverage before
  treating the export as a complete SDK contract.

See [scope and regeneration](docs/contracts/openapi-admission-v1.md).

Remote CI on `8f9a5c9` passed engine contracts, PostgreSQL admission and the actual
backend container build/smoke workflows. This confirms container execution, not
provider provisioning or live staging acceptance.

## Zero-budget development staging — 2026-11-16

The user has approved free plans only; there is no project funding. Prepare Render
Free for the API and Supabase Free for database/Auth/private Storage/queue. Because
Render has no free background-worker plan, use the existing worker on an
operator-controlled development machine for now. This is not always-on hosted
generation: jobs wait while that worker is offline. Preserve engine isolation,
resource limits and separate credentials. Cloudflare frontend/GitHub CLI distribution
choices remain unchanged. No paid resources/add-ons or automatic upgrades are
authorized. Confirm region availability during setup; the earlier paid staging
proposal is superseded. See deploy/README.md for provider limits and quota handling.

## Hosted API schema coverage checkpoint — 2026-11-16

- [x] Typed response envelopes and discovery pagination for the current hosted API.
- [x] Admin operation schemas, confirmation/expected-state inputs and explicit
  operator-granted role metadata without private-content authority expansion.
- [x] All registered routes, health responses and artifact download header contract.
- [x] Real PostgreSQL-backed HTTP responses checked against operation/status schemas;
  export freshness, references, privacy and malformed contract regression checks.

This supersedes the admission-only export checkpoint. Artifact file payload types and
engine error-detail variants remain governed by their separate contracts; OpenAPI
models the JSON download boundary and extensible error envelope explicitly.

## Structured failure evidence checkpoint — 2026-11-16

- [x] Bounded engine GenerationError/counterexamples survive supervised child execution
  and atomic fenced worker finalization without logging raw error text.
- [x] Project-scoped attempt evidence endpoint, immutable persistence, explicit
  truncation, resource-inconclusive semantics and no admin-only content access.
- [x] SQL size/fence/expiry/replay/role/cancellation checks, real worker execution and
  OpenAPI response-contract verification; published artifacts remain untouched.

See [diagnostics contract](docs/contracts/job-diagnostics-v1.md). Overall history
retention/deletion, hosted provider acceptance, load/restore drills and optional AI
telemetry remain open. This completes the previously deferred bounded counterexample
transport, not unrestricted storage of arbitrarily large solver reports.

## Retention and project lifecycle checkpoint — 2026-11-16

- [x] Approved published-until-owner-delete policy, 30-day unsuccessful input/evidence
  retention, seven-day project recovery and 30-day application-backup policy.
- [x] Owner-confirmed delete/restore/discovery endpoints, current-access denial and
  lifecycle epochs that invalidate old worker execution even after restoration.
- [x] Bounded cancellation, queue acknowledgment, payload/key erasure and leased
  explicit-project object purge with partial-failure/late-write recovery.
- [x] Preserve active published designs, shared catalogs, immutable identity records
  and minimum audit history; release project quota after complete content purge.
- [x] Separate offline recovery role for paged lifecycle-ledger export and monotonic
  restore reconciliation; API/worker/routine audit maintenance cannot invoke it.
- [x] Migration-upgrade, HTTP/OpenAPI, real generation/storage and adversarial policy
  tests: 202 passed locally with no skipped tests or relaxed engine deadlines.

See [ADR 0009](docs/decisions/0009-retention-and-project-lifecycle.md),
[contract](docs/contracts/retention-v1.md), [runbook](docs/runbooks/retention-and-recovery.md)
and [tests](docs/testing/retention.md). Live backup scheduling, independent ledger
durability, provider expiry and hosted restore drills remain deployment acceptance
work; no hosted resources or backups were provisioned by this change.

## Docker onboarding and Storage identity checkpoint — 2026-11-17

- [x] Removed unused root environment template; retained component placeholders.
- [x] Docker runbook: offline verified generation/unpack, full service setup, migration
  sequence, post-start workflow, shutdown and troubleshooting.
- [x] Root Supabase guide for the user's deferred free-project preparation.
- [x] User-approved dedicated reader/worker identities: shared server-only session
  login/refresh, bounded/redacted failures, clock/suspend and revoked-token recovery.
- [x] Reviewed provider-specific RLS bootstrap and operator-audited enable/revoke
  registry; live writes/eligible cleanup only, no service-key bypass or overwrites.
- [x] Local 210-test acceptance plus guide shell checks and component lint gates.

The provider SQL is an explicit bootstrap after migration 0016, not migration 0017.
Live account/RLS acceptance waits for the user to prepare Supabase using the guide.
CLI/browser PKCE client work remains next; no GUI work starts early. No hosted
resources, paid plans, real technical accounts or emails were created automatically.

## Hosted CLI login checkpoint — 2026-11-17

- [x] Replaced CLI scaffold with `laby`, independent from engine/API implementation.
- [x] Human email magic-link S256 PKCE, bounded one-use loopback callback, request
  correlation, safe browser completion and cancellation/deadline handling.
- [x] OS credential-store refresh persistence, profile-scoped advisory lock, refresh
  rotation before API use, subject continuity and explicit local-session logout.
- [x] Identity, explicit personal onboarding, tenant/project reads and idempotent
  project creation commands; no implicit workspace creation or plaintext fallback.

See [CLI contract](docs/contracts/cli-v1.md) and [usage](cli/README.md). Provider
resources remain deferred to the user. Local HTTP/credential-adapter tests do not
claim live Supabase/native desktop acceptance. Structured generation, downloads,
complete pagination and release packaging remain subsequent client slices.

## Structured CLI workflow checkpoint — 2026-11-17

- [x] Added bounded, byte-preserving validation/admission envelopes and explicit
  idempotency keys; no duplicate-field normalization or automatic mutation replay.
- [x] Catalog/job/revision discovery with explicit pagination, status/cancellation,
  attempt diagnostics and revision inventory inspection.
- [x] Complete authenticated package download: exact inventory, revision/manifest
  identity and all file sizes/hashes checked before new private output is created.
  Existing outputs are preserved; this is integrity checking, not re-verification.

This completes the initial AI-free CLI command surface and supersedes the prior
pending engineering-command note. Real Supabase/native desktop acceptance, GitHub
release packaging, optional AI and backend operational acceptance remain open.

## CLI distribution implementation — 2026-11-17

- [x] User selected proprietary licensing; recorded project rights separately from
  bundled third-party notices. Public distribution still requires end-user terms.
- [x] Native four-target package builds, extracted-binary smoke tests, normalized
  archive layout, source/version/lockfile/executable provenance and SHA-256 inventory.
- [x] Full-set verification, scoped dependency-audit gate and pinned validation tools;
  read-only builds, bounded artifact retention and tag-only draft prereleases.
- [x] Existing drafts/published versions and moved tags fail closed; no automatic
  release publication, tag creation, asset replacement or privileged installer.

See [release runbook](docs/runbooks/cli-releases.md) and [CLI changelog](cli/CHANGELOG.md).
Native CI acceptance is recorded separately from local Linux smoke. Live Supabase,
native credential-store, signed distribution/end-user terms and broader backend
operational acceptance remain open. Lockfile findings outside CLI scope must be
triaged independently; CLI packaging does not certify workspace-wide security.

## CLI packaging assurance follow-up — 2026-11-17

Native CI validates the real archives and compiler-bound dependency inventory; see
[packaging acceptance](docs/testing/cli-packaging.md) and
[ADR 0011](docs/decisions/0011-native-cli-release-artifacts.md). Windows portability
and cross-platform lock identity defects were corrected without suppressing checks.
Free macOS 15 ARM provisioning failures prompted a documented move to the supported
macOS 26 ARM pool; the deployment target stays 15.0.

Next security work: triage the retained non-CLI lockfile advisories for crypto-bigint,
libsqlite3-sys and rsa using actual component build/affected-operation evidence.
A passing CLI dependency gate does not clear those findings. Live Supabase,
credential-store, operational acceptance, end-user terms and signed distribution
remain their separate gates.


## Backend dependency assurance checkpoint — 2026-11-17

- [x] Reviewed actual separate API/worker/engine compiler inventories and affected
  operations for the retained crypto, RSA and SQLite advisories.
- [x] Upgraded JWT/RustCrypto and ephemeral signing fixtures to patched stable
  versions; retained real ES256/RS256 verification and rejection coverage.
- [x] Replaced the archived catalog YAML dependency with maintained Rust parsing,
  explicit budgets and unchanged pinned catalog identities.
- [x] Added a compiler-bound API/worker/engine/maintenance audit gate, complete
  lockfile evidence, fail-closed tooling tests and a weekly workflow definition.

Local acceptance: 233 Rust tests (real PostgreSQL, all workspace features), 17 Python
checks, formatting and strict Clippy passed. Fresh audit: 261 compiled third-party
versions, zero security or maintenance findings in that scope. Inactive optional
SQLite/RSA advisories and platform maintenance notices remain recorded; no blanket
workspace-wide clean claim or advisory waiver is made. Weekly execution activates
when the workflow reaches `main`; push/PR checks run on this branch now.
See [triage and evidence](docs/security/dependency-triage.md). Supabase provisioning,
live provider/native-store acceptance, backup/restore and broader backend operational
acceptance remain separate open work. The GUI remains deferred.


## Optional AI foundation and frontend design preparation — 2026-11-18

- [x] Added a provider-independent, bounded in-memory proposal/review contract and
  JSON Schema exports. Typed intent parsing rejects duplicate fields before generic
  JSON normalization. Assumptions and configured provider metadata stay separate
  from engine provenance.
- [x] Projected readiness to clarification or `ready_for_review`, with visible
  engine defaults and no unconfirmed `ResolvedIntent` handed out by the boundary.
  Confirmation binds exact content, prompt digest, actor/tenant/project/head/catalog;
  it preserves review identity and does not admit or generate anything.
- [x] Added adversarial fixtures, schema synchronization and redacted Debug checks.
  Full acceptance: 249 workspace Rust tests passed with all features and real local
  PostgreSQL; zero failed/ignored. Strict Clippy and formatting passed.
- [x] Turned frontend/admin requirements into a concrete
  [screen-and-flow brief](docs/product/frontend-workspace-v1.md). This is design
  preparation; visual wireframes/tokens/device artwork and GUI implementation remain.

See [AI boundary](docs/contracts/ai-v1.md). This is not a live intent parser or an
assessment service. No provider SDK/call, persisted AI conversation, AI HTTP route or
frontend code was added. Provider/model and explicit data/retry policies are awaiting
user decisions before live integration. Generation options need a separately bound
review in any future hosted confirm-and-admit path. Supabase/live provider acceptance
remains separate; no existing structured generation path depends on AI availability.
