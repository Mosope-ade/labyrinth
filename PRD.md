# Labyrinth — Product Requirements Document

Version: 1.1 · Date: 2026-11-11
Status: agreed product direction; unresolved details are listed in section 12.
Owner: Adeyinka Adejumo (AdeyLord)
Tagline: From intent to infrastructure.
Implementation roadmap: [plan.md](plan.md)

## 1. Product definition

Labyrinth is a vendor-neutral network topology generator that turns user requirements
into an explainable, verified engineering package. A deterministic engine performs
all network generation. An optional AI interface parses intent into JSON and can
separately assess the completed design. CLI and GUI are independent clients of one
hosted system, using the same backend API, projects and generation workers.

Campus and branch WAN are the first validation scenarios. They are NOT the boundary
of the product. Scenario-specific knowledge belongs in blueprints; topology, device
selection, addressing, policy and verification remain general engine capabilities.

### Problem

Network design requires translating incomplete requirements into architecture,
capacity, equipment capabilities, addressing, routing, segmentation and resilience.
Manual spreadsheets/diagrams can disagree, assumptions can disappear, and plausible
AI-generated designs can lack evidence of feasibility or security isolation.

### Value proposition

A single reproducible model connects engineering requirements, topology, generic
equipment selection and verification evidence. Engineers can inspect why each choice
was made, identify bottlenecks, and evaluate expansion without depending on an LLM
for network correctness.

## 2. Users, goals and exclusions

Primary users: network engineers and architects planning new networks or expanding
existing designs. Secondary users: infrastructure teams reviewing capacity/security
tradeoffs and engineers scripting repeatable designs through the CLI/API.

Goals:
- Produce a complete neutral engineering package from sufficiently resolved intent.
- Select generic device capabilities that meet demand, isolation and resilience needs.
- Expose assumptions, selection reasons, constraints and verification limits.
- Support reproducible redesigns and constrained expansion of existing revisions.
- Offer equal engineering capability through hosted CLI and GUI interfaces.

V1 excludes vendor-specific CLI configs, vendor/SKU-driven selection, runnable virtual
labs, packet emulation, direct device deployment, real-time procurement pricing,
compliance certification and proof of real-device enforcement. Reference hardware
hints are illustrative, not endorsements or a purchase-ready equipment specification.
No offline CLI/desktop mode is committed for V1.

## 3. V1 outputs and user journeys

### Engineering package

A successful generation produces one versioned package containing:
- Vendor-neutral topology graph with devices, interfaces, links, sites and failure domains.
- IPAM/VLAN plan with subnets, reservations, host/gateway assignments and capacities.
- Abstract routing matrix describing required forwarding/next-hop relationships.
- Firewall policy matrix describing permitted/isolated flows and modeled enforcement.
- BOM with generic archetype quantities, operating modes, relative cost tiers and
  optional reference hardware hints.
- Verification report covering all five passes and requested failure scenarios.
- Decision log, resolved intent/defaults and reproducibility/version manifest.

Machine-readable data is authoritative; CSV and human-readable views derive from the
same revision. The core JSON file contract is defined in docs/contracts/core-v1.md; client views derive from it.
An optional AI assessment is a separately labeled, revision-bound artifact.

### New design

1. User signs in through CLI or GUI and creates/selects a project.
2. User provides structured requirements or natural language through the parser.
3. System asks for missing capacity/security-critical information and displays defaults.
4. User selects a supported blueprint or allows rule-based selection.
5. API accepts resolved input as an asynchronous job; user can inspect/cancel it.
6. Engine generates, selects equipment archetypes, verifies and returns evidence.
7. User downloads the published package or receives actionable diagnostics on failure.

### Expansion

User selects Incremental Update against a base revision, adds endpoints/sites and
submits. Existing IPAM allocations and core assignments remain pinned. If expansion
cannot satisfy the pins, system explains the conflict and proposed changes without
altering the previous verified revision. Stateless Redesign is a separate explicit mode.

### AI unavailable

User submits structured JSON and completes the same hosted generation/download flow.
Already-running jobs, saved projects and verified outputs remain available.

## 4. Functional requirements

All FR requirements below are V1 requirements unless explicitly marked as future.

### FR-01 — Intent and clarification

Capture explicit sites, endpoints, peak demand, security zones/flows, redundancy,
blueprint selection, addressing constraints and update mode. Unresolved fields use
Option<T> in partial intent. Never substitute zero/false for missing information.

Clarify missing capacity/security-critical inputs before verified generation. Apply
and record documented secondary defaults: 20% growth headroom and RFC1918 10.0.0.0/8
IPv4 base space. User-supplied exclusions/pins can invalidate the default. Readiness
and validation are deterministic engine/application checks, not AI assertions.

Acceptance: incomplete demand/security input returns named missing fields; valid
structured input requires no AI; every default is visible with source/provenance.

### FR-02 — Blueprint selection

Support explicit blueprint selection and automatic rule-based selection based on
site count, endpoint density and redundancy tier. Allow valid user overrides. Explain
automatic choices and reject unsupported/infeasible overrides rather than replacing them.

First implemented blueprint covers enterprise campus/branch hub-and-spoke scenarios;
three-tier campus and spine-leaf are named extension goals, not implied V1 support.
Core models and interfaces cannot assume every future network has branches or a hub.

Acceptance: automatic and explicit choices are reproducible; unsupported blueprint
requests return a clear error; scenario rules reside outside general engine logic.

### FR-03 — Generic device catalog

Use YAML hardware archetypes with physical and logical capabilities and operating-mode
profiles. Include port counts/speeds/media, relevant restrictions, throughput by mode,
PoE budget, supported logical capabilities, eligible roles and relative cost tier.
Unknown capacity cannot satisfy a hard requirement. Do not select real vendor SKUs.

Acceptance: enabled IPS/VPN changes the applicable throughput limit; incompatible
ports/PoE/features reject; changing reference hints cannot affect engineering selection.

### FR-04 — Device selection and bounded repair

Three steps: blueprint role mapping; hard capability filtering; full-assignment
optimization/ranking. Ranking is strict, in this order:
1. Hard capacity, port and security-isolation constraints.
2. Standardized archetypes across peer roles.
3. Lowest relative cost tier.
4. Spare-port headroom target 20% and minimum device count.

Settled scoring: minimize extra distinct archetypes per DeviceRole peer group, then
sum relative tiers per installed device (not currency), then summed spare-port
shortfall, then device count and stable device/archetype/mode IDs. Growth rounds up
once for raw endpoint counts and peak Mbps; spare-port target is a soft installed-port
fraction, not another demand multiplier. See docs/contracts/selection-v1.md.

Use stable tie-breaking. No vendor affinity. Consider inter-device compatibility and
shared capacity; individual feasible candidates do not establish whole-design feasibility.

Allow at most three topology/selection passes to upgrade archetype tiers or scale
horizontally with switches/uplinks. Preserve pins and reverify derived outputs.

Acceptance: candidate exclusions and winning priorities are explained; hard constraints
are never traded for a cheaper/standardized design; three-pass exhaustion reports a
bottleneck and search limit separately from proven infeasibility. No unsupported claim
of global optimality from heuristic scoring.

### FR-05 — Demand and capacity

Size against declared simultaneous peak demand, not automatic full endpoint line rate.
Include growth and failure-state load. Separate growth allowance from spare-port target.
Operating-mode limits, shared ports and usable aggregate/per-flow paths constrain capacity.

Acceptance: a backup path that preserves connectivity but cannot carry required demand
fails capacity/resilience verification; shared-link demand is not double-counted or ignored.

Per-session/splittable traffic support is in V1 scope. Demand inputs explicitly choose
single-path aggregate reservation or session cohorts (per-session peak × simultaneous
count), with cohort upper bounds summing to the declared aggregate peak. Missing
capacity-critical profile information requires clarification. Individual sessions must
not be striped across paths or justified by summed LACP/ECMP capacity. Controlled session steering with explicit selected-mode capability checks is required;
ordinary hashing is not accepted as a balancing guarantee. Nonzero session growth
explicitly chooses rates or concurrency. Aggregate growth targets and conservative
integer session-granularity reservations must be reported separately.

### FR-06 — Addressing and VLANs

Derive deterministic, capacity-sufficient IPv4 allocations and VLAN assignments from
the canonical design. Represent endpoint networks, routed links, gateways, reserved
space and relevant uniqueness scopes explicitly. Honor exclusions and update pins.

Acceptance: no overlapping allocated peer subnets, duplicate addresses or insufficient
usable host capacity; parent-pool containment is supported; exhaustion is actionable.
IPv6 support is a separately scoped extension, not implied by this IPv4 default.

Agreed addressing conventions are configurable policy inputs, persisted with each
result: the first hub-spoke adapter uses one subnet/VLAN per site zone, reserves one
gateway address for Tier0/1 and three for Tier2/3, /31 routed links, optional /32
loopbacks, and VLAN tags starting at 10 with per-site reuse. Gateway reservation
counts, transit prefix, loopback allocation, VLAN range/exclusions and uniqueness
scope can be overridden. The generic allocator accepts arbitrary named subnet
requests; these scenario defaults must not restrict future topologies. Gateway
reservations do not imply an implemented HA mechanism. Shared-VLAN zones use zone
distribution switches in the first adapter. Redundant Ethernet paths must model
LACP/spanning-tree capabilities and forwarding explicitly; ECMP or a generic HA
flag cannot substitute for an L2 mechanism. See
[the addressing contract](docs/contracts/addressing-v1.md).

Agreed logical-stage contract: deterministic abstract L2 steady-state trees over
explicit LACP adjacencies; active/standby gateway ownership with overridable preference
and no automatic preemption. Both-live total HA-control loss fails closed; confirmed
owner-device failure permits takeover. A live owner losing its modeled local data
path may coordinate transfer to an eligible standby while HA synchronization remains
healthy; total synchronization loss still fails closed. Recovery does not preempt
a committed owner. Reserve two separate HA-control cables per
pair. These model semantics do not claim RSTP timing, real fencing, synchronization
performance, provider reachability or full resilience. See
[logical-v1](docs/contracts/logical-v1.md).

Confirmed WAN boundary: the core requires explicit WanService attachments (site,
provider failure domain, committed bandwidth and connected sites). AI may propose
those values during clarification, but user confirmation/editing precedes generation.
Matching ISP strings alone never prove transport connectivity. Verification is scoped
to the declared abstract service, not carrier internals. See
[routing-policy-v1](docs/contracts/routing-policy-v1.md).

### FR-07 — Abstract routing and firewall policy

Use order-independent deny-overrides with default deny. Reject positive demands
with no permitted service after denies. Derive routes and policy matrices from the
same canonical network. Preserve explicit
allowed flows and required isolation; model enforcement locations and routing choice.
No vendor config text is produced. Reject unsupported policy semantics rather than
silently treating inspection as permit or a physical path as an allowed flow.

Acceptance: required allowed flows are reachable and isolated flows have no permitted
modeled path; an all-deny solution cannot satisfy allowed-traffic requirements.
Detailed protocol/stateful semantics must be frozen before implementation (section 12).

### FR-08 — Redundancy tiers

- Tier0_None: single links/devices allowed, no failure-survival promise.
- Tier1_Link: dual uplinks (modeled LACP/ECMP); relevant single-link loss preserves
  required connectivity/capacity. Devices may remain single.
- Tier2_Device: Tier1 plus dual-homed access, HA firewalls and redundant core/distribution
  as relevant. Protect required backbone paths among surviving devices. Single-homed
  endpoint attachment is explicitly outside the single-device guarantee.
- Tier3_Site: Tier2 plus dual-WAN/multi-ISP and geo-redundant hubs. Require a second
  hub location and independent connectivity; ask for clarification when absent.

Acceptance: remove each relevant link/device, plus site/ISP/circuit failure domains
for Tier3. Check capacity, routing and security after failure. Shared upstream/provider
infrastructure cannot count as independence. Failed site's own endpoints are not
promised survival without modeled endpoint/application replication.

### FR-09 — Five-pass verification

Every result includes deterministic evidence for:
1. Structural: intended connectivity, references, port-speed/media matching and no
   uplink overload under declared demand.
2. Addressing: allocation integrity and adequate host capacity.
3. Routing: loop-free modeled forwarding and valid next-hop reachability for allowed flows.
4. Security: formal reachability check over modeled routes, packet classes and policies,
   proving isolated zones have zero permitted paths.
5. Resilience: failures matching the requested tier, with protected-set/exclusion evidence.

Acceptance: pass/fail/inconclusive is explicit; failures identify a counterexample or
bottleneck; timeouts never pass. Tier0 reports no survival guarantee. Evidence states
its abstraction limits and cannot be presented as a certificate of real deployment safety.

### FR-10 — Change modes and revisions

Stateless Redesign recomputes from requirements. Incremental Update locks existing
IPAM allocations/core assignments as constraints for new endpoints/sites. All results
are immutable revisions; stale base revisions are rejected. Changes to pins require
an explicit new user instruction, never automatic relaxation.

Acceptance: valid expansion preserves pins; conflicting expansion returns diagnostics;
failed generation leaves the last good revision intact; historical revisions stay readable.

### FR-11 — Catalog governance and reproducibility

Generate strict schema through schemars and validate bundled YAML at build/test time;
run semantic validation too. Catalog bundles carry semantic version plus SHA-256 hash.
Every project revision pins and retains its exact catalog snapshot.

Same normalized resolved inputs, pinned base revision/catalog and engine/rule/search
versions produce the same design and deterministic package payloads. Separate timestamps,
job IDs and AI-assessment metadata from deterministic engineering content.

Acceptance: repeated builds match; edited catalog content changes its digest; mismatched
hashes reject; new catalog releases cannot silently change old designs or updates.

### FR-12 — Hosted jobs and publication

Axum accepts generation asynchronously. Persist jobs/attempts and immutable inputs;
workers execute the engine with bounded resources. Support cancellation, idempotency,
restart recovery and authoritative status retrieval after missed stream events.

Initial solver invocation safeguard: 15-second maximum, separately configurable from
total job timeout and deterministic search budget. Timeout is inconclusive; bounded
search failure is not automatically proven infeasibility. Stop computation, not only
its awaiting request, when enforcing limits.

Acceptance: no lost accepted job after restart; no duplicate published revision from
retries; failed/cancelled/inconclusive execution exposes diagnostics but no verified
partial package. Object staging and DB manifest publication handle storage failures.

### FR-13 — Identity and tenant isolation

Authenticate CLI/GUI/API users. Enforce ownership across projects, revisions, jobs,
intent sessions, events, assessments and artifact access. Store objects privately;
download authorization is checked before issuing access. Network design data is sensitive.

Acceptance: user A cannot read/mutate/download/subscribe to user B's resources by ID;
logs omit sensitive payloads; deleted access cannot be bypassed through public object URLs.
User-access-token verification and HTTP authorization are defined in ADR 0005.
Interactive login/refresh, scripted credentials and provisioning remain staged decisions.

### FR-14 — Optional AI

AI parses requirements to schema-validated JSON; it does not generate networks. It
may assess a completed immutable design on request, returning a separate report.
No assessment changes graph, BOM, IPAM, policy, verification or export eligibility.
Structured JSON is always a supported path when the provider is unavailable.

Acceptance: invalid AI output cannot enter generation; missing critical fields clarify;
assessment records source revision and provider/model; provider failure leaves raw-intent
generation and existing results usable. No AI engineering conclusion is treated as proof.

### FR-15 — CLI and GUI parity

CLI and GUI are independently built clients of the same API. Both support structured
input, clarification, generation jobs, result inspection and package downloads. CLI
supports scripting/machine-readable output. GUI implementation follows backend readiness.

Acceptance: equivalent resolved requests produce equivalent results across clients;
no client contains an alternative generator; CLI V1 generation requires the hosted service.

### FR-16 — Retention and deletion

Define automatic cleanup for sensitive artifacts, staging objects, diagnostics, job
metadata and AI messages. Preserve required catalog snapshots with retained revisions.
Support project deletion and document backup/deletion windows. Retention rules must
not remove the only reproducibility inputs of a still-retained design.

Acceptance: expired staging artifacts are inaccessible/cleaned; project deletion
revokes access; cleanup is recoverable/idempotent. Exact periods are release blockers.

### FR-17 — Frontend stack and engineering workspace

Build the hosted GUI separately using React + TypeScript, React Flow for the topology
canvas, Tailwind CSS and shadcn/ui. Use @monaco-editor/react for JSON/YAML intent,
neutral artifact inspection/diffs and admin catalog drafts. Generated artifacts are
read-only; no vendor config viewer is implied. Load canvas/editor functionality only
where needed and manage editor/worker lifecycle to avoid leaks.

Provide a three-pane workstation: left requirements/AI clarification/project-site tree;
flexible center topology canvas; right context-sensitive inspector for archetypes,
ports, IPAM/VLANs, routing, policies, BOM, decisions and verification evidence. Side
panels are resizable/collapsible with bounded widths; ultrawide space primarily expands
the canvas. Non-canvas project/settings/admin pages use centered, readable containers.

Acceptance: users can complete structured generation without chat, inspect every
package component, recover async status after reconnect, and see accurate empty,
loading, failed, cancelled, inconclusive and verified states. No optimistic verified
status before backend publication.

### FR-18 — Visual layout and device vocabulary

Visual layout is independent of network generation. Positions, grouping, edge routes,
viewport and collapsed groups are presentation state keyed by stable revision/entity
IDs; dragging never changes topology, IPAM, requirements or verification. V1 canvas
supports layout and inspection only. Engineering edits go through requirements and
engine regeneration, not direct graph mutation.

Evaluate a dedicated layout engine against grouped multi-site and parallel-link
fixtures before choosing it. Required behavior: site/zone grouping, readable labels,
distinguishable HA pairs and link bundles, fit selection/reset layout, preserved manual
positions and stable viewport during progress updates. Large layout work must not
block interaction; stale layout responses cannot overwrite a newer revision.

Create an original vendor-neutral icon/node system for routers, switches, firewalls,
endpoints, wireless and external networks. Distinguish sites, zones, HA pairs and bundles.
Use labels/symbols as well as color; add detail as zoom permits. Link failures and
verification counterexamples highlight the relevant path/components. Failure overlays
show backend evidence or request analysis; they do not invent new verification claims.

Acceptance: rearranging a verified graph preserves its engineering digest/status;
reloading restores saved layout; changing revision cannot attach old positions/evidence
to unrelated devices; no hidden delete/connect action alters engineering constraints.

### FR-19 — Viewport support and accessibility

Below 768 CSS pixels of viewport width, replace both workspace and admin app with:
"Designed for desktop, laptop, and tablet workspaces. Please open on a larger screen".
Use available width, not user-agent detection. Preserve drafts and jobs across resize.

At tablet widths (768–1023px), use collapsible overlay drawers and touch-friendly
controls. At 1024px and above, progressively expose bounded side panes; at 1280px and
ultrawide widths, keep text/forms readable and let the canvas absorb remaining space.
Wide data views may expand deliberately; never stretch all text across the monitor.
Keep scrolling inside appropriate panels rather than overflowing the workstation shell.

Provide accessible controls, visible focus, keyboard navigation, non-color status cues,
reduced-motion support, and a list/inspector alternative to canvas-only inspection.
Generation/pan shortcuts are scoped and documented; they must not intercept Monaco
editing, text fields or browser conventions. Tablet drawers manage and restore focus.

Acceptance: test phone fallback, 768/1024/1280 boundaries, common laptop/tablet sizes,
ultrawide screens and zoom. No horizontal shell overflow, inaccessible primary action,
lost input on resize, or keyboard-only dead end. Touch targets remain usable.

### FR-20 — Admin operations and AI usage

Provide an authenticated admin dashboard with operational visibility: job/queue counts,
execution duration, failure categories, retries, search exhaustion and timeout rates.
Show solver-specific metrics only when that solver is used. Separate AI parse errors,
engineering conflicts, proven infeasibility, exhausted searches and inconclusive runs.

AI monitoring covers tokens, latency and estimated spend by user and globally, with
provider/pricing provenance and unavailable-data states. Redacted failure examples
require explicit privacy/access controls; raw user prompts are not exposed by default.

Acceptance: metrics reconcile to job/usage records for stated time windows; no fabricated
telemetry or exact-cost claims; routine admin views do not reveal sensitive designs.

### FR-21 — Admin catalog lifecycle

Inspect active semantic version/hash, loaded YAML archetypes and selection frequencies.
Validate catalog drafts in an isolated preview, including schema/semantic checks and
regression scenarios. Publishing creates an immutable catalog bundle; changing the
default affects future builds only. A rollback switches the default to a retained
version without mutating existing revisions. Monaco edits drafts, never published YAML.

Acceptance: invalid drafts cannot publish; publishing/rollback records actor and
version/hash; concurrent publication conflicts safely; historical projects retain
exact catalog snapshots; selection frequency does not automatically alter ranking.

### FR-22 — Admin users, jobs and audit trail

Provide user/project/storage usage and quota visibility, account suspension, and
controlled cancellation/recovery of stalled jobs. Recovery creates a recorded attempt
or new job without erasing history or duplicating published revisions. Account policy
changes cannot silently modify engineering results or disclose private artifacts.

Enforce User/Admin authorization on the backend and every admin operation; hidden
navigation is not security. Prevent client-controlled role escalation. Sensitive
mutations require explicit confirmation and attributable, append-only audit records
covering actor, target, action, time, outcome and relevant old/new values without secrets.
Operational application roles cannot edit/delete audit history. Define audit retention
and narrowly authorized infrastructure access separately.

Acceptance: ordinary users receive authorization failures on admin APIs; suspension
is enforced server-side; cancellation/recovery respects worker leases; privileged
mutations are auditable; admin status alone is not unrestricted access to private
project contents. No unaudited impersonation or destructive bulk operation in V1.

## 5. Architecture and deployment requirements

Separate components: hosted web GUI, installed CLI, Axum API, generation workers,
pure Rust engine, authentication integration, database and private object storage.
The API handles application state and orchestration; workers invoke the engine and
persist its outputs. Engine unit/fixture tests remain independent of infrastructure.

Workers and API are separately runnable and scalable; independent components do not
require independent physical hosts initially. Queue durability may be implemented
through the selected database or a queue service; Supabase Queues (pgmq) is selected; see ADR 0003.

Development must provide an AI-free integration environment. Before release, define
region, service credentials, transport/storage protection, limits, backups/restore,
observability and cleanup operations. No desktop/offline, provider choice, pricing,
or production deployment is implicitly authorized by this PRD.

## 6. Nonfunctional requirements

- Correctness: no silent requirement loss, successful placeholder outputs or false proofs.
- Determinism: versioned inputs, stable ordering/tie-breaking, reproducible artifacts.
- Explainability: trace each major decision and failed constraint to model components.
- Resource safety: bounded passes, solver effort/time, total job CPU/memory/time,
  topology size and user concurrency; no unbounded background solver on cancellation.
- Reliability: durable accepted jobs, idempotent retries, atomic result visibility,
  recoverable storage publication and immutable successful revisions.
- Privacy: least-privilege access, tenant isolation, private artifacts, redacted logs,
  explicit external AI data handling and tested deletion/retention.
- Extensibility: blueprints/archetypes extend the general engine without campus-only
  assumptions or vendor-specific device-selection logic.
- Testability: deterministic fixtures, negative cases, counterexamples and independent
  reference checks for selected algorithms. Algorithms/crates in the reference notes
  are optional candidates; custom approaches require the same evidence.

No latency/throughput/availability SLO beyond the stated solver safeguard is agreed.
Set production SLOs after workload/scale fixtures; do not invent marketing guarantees.

### Explicit engineering quality baseline

Scalability, security, reliability and maintainability are baseline requirements, not
post-launch enhancements. No issue-free guarantee is claimed: prevent predictable
failures, detect problems and recover safely with tested evidence.

- Scalability: independent API/worker scaling, bounded queues/payloads/concurrency,
  pagination, indexed access patterns, bounded client data/rendering work and measured
  capacity tests. Benchmark representative workloads before setting numeric SLOs.
- Security: least privilege, deny-by-default authorization, server-side ownership/RBAC,
  schema/semantic validation at trust boundaries, parameterized database access,
  safe output rendering, protected secrets, redacted logs and private artifacts.
  Define threat models for parsing, catalog publication, jobs, admin and downloads.
- Reliability: explicit state machines, bounded retries with backoff for transient
  failures, idempotency, cancellation, durable recovery, atomic visibility, health
  signals, structured telemetry, backups and tested restore/cleanup procedures.
- Standards: strict TypeScript; Rust formatting/lints; typed public boundaries and
  structured errors; small cohesive modules; documented versioned API/schema contracts;
  explicit units and stable identifiers; accessible UI and consistent design tokens.
  Avoid unchecked casts, hidden fallbacks and unsafe code without justified review.
- Verification: behavior/property/integration tests, security negative tests, browser
  runtime checks when UI exists, dependency/secret scanning, migration/rollback checks
  and code review. Lock dependencies and assess changes rather than bulk upgrading.
  Never silence checks, reduce assertions or disable tests to obtain a green result.

Acceptance: each implementation slice records relevant checks and risks; CI enforces
applicable checks; release evidence includes load/resource limits, authorization tests,
worker/storage failure recovery, and documented unresolved limitations. Security and
performance claims must name the tested scope instead of asserting universal safety.

## 7. Initial validation scenarios

These scenarios test the product; they do not define its ultimate domain.

- Baseline: one hub and N branches with declared demand, VLAN segmentation, HA
  firewall requirements and dual-ISP edge as requested by the selected tier.
- Tier0/1/2 variants: include precise protected paths and attachment exclusions.
- Tier3 variant: two geographic hubs and independent connectivity; one-hub input
  triggers clarification instead of a false site-resilience claim.
- Expansion: add endpoints/branch to a pinned revision, both feasible and conflicting.
- Capacity stress: operating-mode derating, incompatible ports, PoE exhaustion,
  overloaded post-failure links and three-pass selection exhaustion.
- Security: permitted traffic, required isolation, alternate bypass paths, and a
  deliberate counterexample proving a failing topology is rejected.
- Hosted failures: missing AI key, solver timeout, worker restart, concurrent updates,
  tenant-crossing requests, unavailable storage and cancelled publication.

Additional blueprints enter a published support matrix with explicit maturity and
verification coverage. A blueprint name in a schema is not evidence of implementation.

## 8. Acceptance and success measures

V1 is accepted when all FR requirements have test evidence and:
- A structured intent completes the hosted API/CLI workflow without an AI key.
- Every successful revision contains the complete neutral engineering package and
  five-pass evidence; no vendor config or runnable-lab dependency is required.
- Repeat builds match on deterministic content with pinned inputs/catalog versions.
- All four resilience tiers are covered by positive and negative scenario fixtures.
- Security isolation failures return counterexamples and never verified status.
- Incremental updates preserve pins; conflicts cannot corrupt existing revisions.
- Restart/cancellation/timeout/storage failures and cross-tenant attempts are tested.
- Workspace/admin behavior passes FR-17–FR-22, including mobile guard, access control,
  catalog lifecycle, auditability and visual-layout/engineering separation.
- Required pre-release decisions in section 12 are settled and documented.

Track: verified completion rate for supported complete intents; clarification frequency;
proven-infeasible versus exhausted/inconclusive outcomes; job latency/resource usage by
scenario size; reproducibility failures; tenant-isolation failures; and report completeness.
Production numeric targets follow benchmarks. Evaluation uses labeled fixtures and
known counterexamples, not AI agreement with the engine's own output.

## 9. Release sequencing

Backend first: contracts → catalog/selection → full engine slice → resilience/update
modes → hosted async API/workers/storage/auth → CLI/optional AI → backend acceptance.
GUI follows the proven contracts. Admin APIs, authorization, telemetry and catalog
lifecycle are backend work and must be designed before the admin screens. Backend
acceptance gates cover backend-applicable requirements; full V1 release additionally
requires GUI/admin acceptance. Frontend polish must not mask backend gaps.
Detailed tasks, dependencies and gates are maintained in plan.md.

Vendor configuration rendering, virtual labs, offline execution and additional
blueprints are separate future scope decisions, not obligations from the deleted code.

## 10. Risks and controls

- Generic archetypes may look more precise than their evidence: publish assumptions,
  operating limits and informational-only hardware hints.
- Formal claims may exceed the model: enumerate supported semantics/exclusions and
  return inconclusive for unsupported proof obligations.
- Heuristic feedback may miss feasible designs: distinguish search exhaustion from
  infeasibility and show attempted repairs.
- Redundancy can hide shared dependencies: model explicit failure domains and demand
  after failover, not simply count parallel links.
- Incremental pins can block growth: preserve them and propose changes explicitly.
- AI may fabricate critical inputs: preserve unknowns and require clarification.
- Catalog changes may invalidate repeatability: archive and pin exact content hashes.
- Sensitive packages may leak: enforce private storage, ownership and cleanup policies.

## 11. Agreed decisions and terminology

Agreed: vendor-neutral archetypes; general physical/logical features with operating
modes; AI parser plus optional read-only assessment; hosted CLI/GUI sharing the API;
separate engine/API/clients; database/auth/object storage; relative cost tiers;
clarification of critical gaps; documented secondary defaults; three-step selection;
three-pass repair bound; strict ranking; five verification passes; four failure tiers;
declared simultaneous peak demand; backbone Tier2 coverage; second-hub clarification
for Tier3; Stateless Redesign/Incremental Update; semantic version + SHA-256 catalog
pinning; bounded async hosted jobs; backend-first delivery; scenario-neutral core.

Archetype: generic hardware capability profile. Blueprint: versioned architecture
rules deriving roles/connections. Requirement: desired user outcome. Default: explicit
engine-supplied secondary value. Pin: constraint preserving prior allocation/assignment.
Verified: required modeled properties passed with evidence. Inconclusive: insufficient
proof/computation, not success. Reference hint: optional real-equipment example, not a
selected vendor device. Engineering package: one complete, revision-bound result.

## 12. Open decisions and release blockers

Coding may begin with workspace scaffolding, typed boundary/fixture tests and CI.
Freeze the relevant contract before implementing behavior that depends on an open
choice below. These are staged decisions, not permission to invent product behavior.


Do not treat these as agreed defaults. Resolve each before the named implementation:

Core decisions settled during implementation:
- Directional stateless protocol/port policies, explicit return traffic, deny-overrides.
- Aggregate single-path traffic or controlled session cohorts with explicit growth
  dimension and required steering capabilities; no guaranteed ordinary hash balancing.
- Explicit reciprocal WAN service attachments; provider names alone never connect sites.
- Scoring/headroom, configurable addressing, zone distribution and conservative HA
  ownership/path-health failover are recorded in docs/contracts/.
- Complete JSON package/options/report/error contracts and bounded search/scale limits
  are defined in docs/contracts/core-v1.md and generated schemas.
- The first hub-spoke scenario supports Tier0–Tier3. Other blueprints reject explicitly.

Before hosted implementation:
- Provider selection is settled in ADR 0003: Cloudflare Pages, Render, Supabase
  Postgres/Auth/Storage/Queues and GitHub Releases. Exact regions, service sizing and
  API/CLI interactive/scripted authentication contracts remain to be finalized.
- Total job CPU/memory/time limits, queue/concurrency quotas and bounded retry policy.

Before AI integration:
- Provider/model, parser correction/retry behavior, assessment context redaction and
  consent/data handling, conversation/session persistence and retention.

Before GUI implementation:
- Visual tokens/typeface, detailed node/icon designs, layout-engine choice and measured
  graph-size budgets. React/TypeScript, React Flow, Tailwind, shadcn/ui and Monaco
  are settled; no unselected framework/bundler or layout library is implied.

Before admin mutation endpoints:
- Role provisioning/revocation and suspension semantics, sensitive-operation confirmation,
  audit retention/access, safe catalog promotion and stalled-job recovery contracts.

Before release:
- Retention durations, deletion/backup windows, operational SLOs, restore targets and
  workload limits measured against representative scenarios.
- How reference hardware hints are curated/reviewed without becoming vendor selection.

These decisions refine implementation; they cannot reintroduce vendor-specific
selection, mandatory AI, local-only CLI, or scenario-specific product scope silently.
