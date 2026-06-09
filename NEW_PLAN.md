# Labyrinth — Full Product Plan

> *"From intent to infrastructure."*

A conversational network design compiler that takes natural language input and produces verified topologies, IP plans, vendor configurations, security reports, interactive diagrams, and exportable lab environments.

---

## Table of Contents

1. [Project Identity](#1-project-identity)
2. [Product Vision](#2-product-vision)
3. [Core Philosophy](#3-core-philosophy)
4. [System Architecture](#4-system-architecture)
5. [Tech Stack](#5-tech-stack)
6. [Repository Structure](#6-repository-structure)
7. [Intent Schema Design](#7-intent-schema-design)
8. [Data Model — Topology Graph](#8-data-model--topology-graph)
9. [AI Layer — Behavior & Boundaries](#9-ai-layer--behavior--boundaries)
10. [Network Rule Engine](#10-network-rule-engine)
11. [Security Policy DSL](#11-security-policy-dsl)
12. [Vendor Capability Model](#12-vendor-capability-model)
13. [Config Generation](#13-config-generation)
14. [IPv4 & IPv6 Addressing Engine](#14-ipv4--ipv6-addressing-engine)
15. [Intelligent Labelling System](#15-intelligent-labelling-system)
16. [Output Specification](#16-output-specification)
17. [Interactive Features](#17-interactive-features)
18. [Lab Export Strategy](#18-lab-export-strategy)
19. [UI/UX & Product Experience](#19-uiux--product-experience)
20. [AI Audit System](#20-ai-audit-system)
21. [Trust & Auditability](#21-trust--auditability)
22. [Failure Handling Strategy](#22-failure-handling-strategy)
23. [Versioning System](#23-versioning-system)
24. [Scale Strategy](#24-scale-strategy)
25. [Build Phases](#25-build-phases)

---

## 1. Project Identity

| Property | Value |
|---|---|
| Product name | Labyrinth |
| CLI name | `laby` |
| GitHub | github.com/Mosope-ade/labyrinth |
| Staging URL | labyrinth.vercel.app |
| Production URL | labyrinth.network |
| Repo structure | Monorepo — Cargo workspace |

**Tagline:** *"From intent to infrastructure."*

**What the name means:** A labyrinth is a network — interconnected paths, decision points, zones. The product makes complex network design navigable.

---

## 2. Product Vision

Labyrinth is a **Network Lab Generator and Network Design Compiler**.

### The user experience in one paragraph

A network engineer types: *"Design a secure 3-site OSPF network with DMZ and redundancy for a hospital"* — and Labyrinth responds conversationally, asks targeted clarifying questions, then builds a verified network design in real time: topology appears node by node, IP addressing fills in, routing protocol configs generate, security rules evaluate, and a professional architecture diagram renders. The engineer can verify zone policies in the browser, download a GNS3 project, or export documentation — all without writing a single line of config manually.

### What it is NOT

- Not a network simulator (no actual packet forwarding)
- Not an AI config generator (AI never writes configs directly)
- Not a Cisco-only tool (multi-vendor, abstraction-first)
- Not a static diagram tool (everything is interactive and derived from a live graph)

---

## 3. Core Philosophy

### Determinism above all

Same input must always produce the same output. The network design engine is code, not AI. AI is a natural language interface and an optional auditor — not the builder.

```
AI interprets → Code decides → Templates generate → Engine verifies
```

### Security is not optional

Security enforcement is a hard gate in the pipeline, not a cosmetic layer. Insecure designs cannot be exported in production mode. Every design decision is explainable and logged.

### One source of truth

The Topology Graph is the single source of truth for every output. IP plans, configs, diagrams, GNS3 exports, simulation views, and audit reports are all derived from the same graph. Change the graph, everything regenerates consistently.

### No partial outputs

Either a fully valid, verified network is produced or the pipeline is blocked with a specific, actionable error. No half-generated configs. No silent failures.

---

## 4. System Architecture

### Full Pipeline

```
User (natural language)
        ↓
AI Conversation Layer         ← dynamic, multi-turn, persistent context
        ↓
Intent Schema                 ← strict validated Rust struct
        ↓
Rule Engine — Phase 1         ← Structural rules → Graph v1
        ↓
Rule Engine — Phase 2         ← Routing rules → Graph v2
        ↓
Rule Engine — Phase 3         ← Segmentation rules → Graph v3
        ↓
Rule Engine — Phase 4         ← Addressing rules → Graph v4
        ↓
Rule Engine — Phase 5         ← Service rules → Graph v5
        ↓
Rule Engine — Phase 6         ← Security rules → Graph v6
        ↓
Rule Engine — Phase 7         ← Optimization rules → Graph v7 (final)
        ↓
Security Policy Engine        ← DSL evaluation → violations → fixes → Graph v8
        ↓
Capability Resolution Engine  ← vendor capability validation
        ↓
Config Generator              ← minijinja templates via vendor adapters
        ↓
Verification Layer            ← automated structural + consistency tests
        ↓
Output Layer                  ← all artifacts derived from final graph
        ↓
AI Audit Layer                ← optional, user-triggered, read-only
```

### Key Architectural Principle — Immutable Graph Versions

The graph is never mutated in place. Every rule execution and every security fix produces a **new graph version**. This means:

- Every state of the network is preserved and inspectable
- The audit trail is exact — Graph v3 shows the network after segmentation rules, Graph v6 after security rules
- Bugs are reproducible — you can replay from any graph version
- Rollback is trivial — revert to any prior version

### Four Core Systems

| System | Role | Implementation |
|---|---|---|
| AI Layer | Interpretation, labelling, auditing | Anthropic API (Claude) |
| Rule Engine | Graph construction, 7-phase deterministic transforms | Rust code |
| Security Policy Engine | DSL evaluation, violation detection, remediation | Rust code |
| Capability Resolution Engine | Vendor capability validation before config generation | Rust code |

These systems are strictly separated. The AI cannot modify the graph. The security engine cannot be bypassed in production mode. The rule engine never calls vendor-specific code. Config generation only begins after capability validation passes.

---

## 5. Tech Stack

| Layer | Technology | Reason |
|---|---|---|
| Frontend | Next.js + TypeScript | Familiar stack, SSR, ecosystem |
| Topology UI | React Flow | Purpose-built for node/edge graphs |
| Desktop shell | Tauri v2 | Shares Rust core, single binary |
| Backend API | Axum (Rust) | Performance, safety, same language as core |
| Graph engine | petgraph (Rust) | Native graph traversal, well-maintained |
| Config templates | minijinja (Rust) | Jinja2-compatible, pure Rust |
| Schema validation | serde + validator | Standard Rust deserialization + rules |
| AI intent parsing | Anthropic API (Claude) | Best schema-adherent structured output |
| Database | PostgreSQL via sqlx | Reliability, async, type-safe queries |
| Async jobs | apalis or tokio tasks | Heavy generation jobs off the main thread |
| Auth | JWT via axum middleware | Stateless, scales horizontally |
| Styling | Tailwind CSS | Consistent with existing workflow |

### Desktop Architecture (Tauri)

```
Next.js frontend
      ↓
Tauri v2 shell (desktop target)      ← same frontend, two targets
      ↓
netgen-core (Rust library)           ← shared with API server
```

The core engine is a Rust library compiled into both the Tauri desktop app and the Axum API server. Zero logic duplication.

---

## 6. Repository Structure

Monorepo using a Cargo workspace for Rust crates alongside a Next.js app.

```
labyrinth/
│
├── Cargo.toml                    # Cargo workspace root
│
├── netgen-core/                  # Pure Rust library — no HTTP, no DB
│   ├── src/
│   │   ├── schema/               # Intent schema types (serde structs)
│   │   ├── graph/                # Topology graph (petgraph wrapper + versioning)
│   │   ├── rule_engine/          # 7-phase network decision engine
│   │   │   ├── phases/           # One module per phase
│   │   │   ├── rules/            # Individual rule implementations
│   │   │   └── packs/            # Rule packs (Enterprise, Lab, Cloud, ISP)
│   │   ├── security/             # Security Policy DSL + evaluator
│   │   │   ├── policies/         # Policy definitions
│   │   │   ├── remediations/     # Remediation rules (separate from policies)
│   │   │   └── packs/            # Policy packs (HIPAA, PCI-DSS, Zero Trust)
│   │   ├── capability/           # Vendor Capability Resolution Engine
│   │   │   ├── profiles/         # VendorProfile definitions per vendor
│   │   │   └── contracts/        # Capability contract evaluation
│   │   ├── addressing/           # IPv4 + IPv6 allocation
│   │   ├── labelling/            # Device + zone naming logic
│   │   ├── templates/            # minijinja config templates
│   │   │   ├── cisco/ios/        # Cisco IOS-XE templates
│   │   │   └── juniper/junos/    # Juniper JunOS templates
│   │   ├── adapters/             # VendorAdapter implementations
│   │   │   ├── cisco.rs
│   │   │   └── juniper.rs
│   │   ├── verification/         # Post-generation validation
│   │   ├── simulation/           # Route calculator, path tracer, CLI renderer
│   │   └── export/               # GNS3, diagram, IP plan, docs exporters
│   └── Cargo.toml
│
├── netgen-api/                   # Axum web server — calls netgen-core
│   ├── src/
│   │   ├── routes/               # API endpoints
│   │   ├── middleware/           # Auth, rate limiting
│   │   ├── jobs/                 # Async job handlers
│   │   └── db/                   # sqlx queries
│   └── Cargo.toml
│
├── netgen-desktop/               # Tauri app — calls netgen-core
│   ├── src-tauri/
│   └── Cargo.toml
│
├── netgen-web/                   # Next.js frontend
│   ├── app/
│   ├── components/
│   │   ├── topology/             # React Flow canvas
│   │   ├── diagram/              # Architecture diagram viewer
│   │   ├── simulation/           # Simulation view components
│   │   ├── chat/                 # AI conversation interface
│   │   ├── configs/              # Config viewer + syntax highlighting
│   │   └── audit/                # AI audit report panel
│   ├── lib/
│   └── package.json
│
├── docs/                         # Product documentation
└── README.md
```

**Key principle:** `netgen-core` has zero knowledge of HTTP, databases, or the frontend. It is a pure library of network engineering logic. Both `netgen-api` and `netgen-desktop` depend on it as a Cargo workspace member.

---

## 7. Intent Schema Design

The intent schema is the **contract** between the AI layer and the network engine. It is the only thing the AI is allowed to produce directly. It must be expressive enough to describe any network but strict enough to prevent ambiguity.

### Full Schema (Rust structs)

```rust
// Root intent — everything starts here
struct NetworkIntent {
    metadata:     IntentMetadata,
    topology:     TopologyIntent,
    routing:      RoutingIntent,
    segmentation: SegmentationIntent,
    addressing:   AddressingIntent,
    services:     ServicesIntent,
    constraints:  ConstraintsIntent,
    context:      ContextIntent,       // industry, compliance, labels
}

struct IntentMetadata {
    name:             String,
    environment:      Environment,     // Lab | Staging | Production
    vendor_preference: Vec<Vendor>,    // ordered: ["cisco", "juniper"]
    security_level:   SecurityLevel,   // Basic | Standard | Hardened
    schema_version:   String,          // "intent_schema_v1"
}

struct TopologyIntent {
    sites:            u8,
    topology_type:    TopologyType,    // HubSpoke | FullMesh | PartialMesh | PointToPoint
    redundancy_level: RedundancyLevel, // None | Standard | High
}

struct RoutingIntent {
    protocols: Vec<RoutingProtocol>,   // Ospf | Bgp | Static | IsIs
    ospf:      Option<OspfConfig>,
    bgp:       Option<BgpConfig>,
}

struct OspfConfig {
    areas:          u8,
    authentication: bool,
    area_types:     Vec<AreaType>,     // Backbone | Stub | Nssa | Totally Stub
}

struct BgpConfig {
    enabled:    bool,
    bgp_type:   BgpType,              // Internal | External
    local_as:   Option<u32>,
    peer_as:    Option<u32>,
}

struct SegmentationIntent {
    dmz:        bool,
    vlan_count: u8,
    zones:      Vec<ZoneIntent>,
}

struct ZoneIntent {
    name:        String,              // user-provided or AI-inferred
    role:        ZoneRole,            // Lan | Wan | Dmz | Mgmt | Transit | Oob | Guest
    trust_level: u8,                  // 0–100, engine derives policy from delta
}

struct AddressingIntent {
    mode:  AddressingMode,            // Ipv4Only | DualStack | Ipv6Only
    ipv4:  Option<Ipv4Config>,
    ipv6:  Option<Ipv6Config>,
}

struct Ipv4Config {
    base_range: String,               // e.g. "10.0.0.0/8"
    scheme:     AddressingScheme,     // CidrOptimized | Classful
}

struct Ipv6Config {
    ula_prefix: Option<String>,       // auto-generated per RFC 4193 if absent
    gua_prefix: Option<String>,       // real allocation if they have one
    link_local:  bool,                // always true, non-negotiable
}

struct ServicesIntent {
    internet_gateway: bool,
    vpn:              bool,
    nat:              bool,
    dns:              bool,
    ntp:              bool,
}

struct ConstraintsIntent {
    no_single_point_of_failure: bool,
    strict_security:            bool,
    lab_mode:                   bool, // relaxes security to WARN only
}

struct ContextIntent {
    industry:           Option<Industry>,   // Healthcare | Education | Retail | Industrial | Enterprise
    compliance_profiles: Vec<Compliance>,   // Hipaa | PciDss | Iso27001
    departments:        Vec<String>,        // raw names, AI provides these
    custom_labels:      Vec<CustomLabel>,   // user-specified overrides
}
```

### Schema Versioning

Every intent struct carries `schema_version`. When the schema changes, a migration function upgrades old intents forward. Breaking changes increment the major version. Old projects never silently break.

---

## 8. Data Model — Topology Graph

The topology graph is the **single source of truth** for the entire system. Every output artifact is derived from it.

```rust
struct TopologyGraph {
    nodes:      Vec<NetworkNode>,
    edges:      Vec<NetworkLink>,
    zones:      Vec<SecurityZone>,
    policies:   Vec<ZonePolicy>,
    addressing: AddressingPlan,
    decisions:  Vec<DesignDecision>,    // audit trail, always populated
    metadata:   GraphMetadata,
}

struct NetworkNode {
    id:         Uuid,
    label:      String,                 // e.g. "RTR-CORE-01"
    role:       NodeRole,               // Router | Switch | Firewall | Host | Cloud | Ap
    vendor:     Vendor,
    zone:       ZoneRef,
    interfaces: Vec<Interface>,
    config:     Option<DeviceConfig>,   // populated after config generation
}

struct NetworkLink {
    id:        Uuid,
    from:      (NodeRef, InterfaceRef),
    to:        (NodeRef, InterfaceRef),
    bandwidth: Option<u32>,             // Mbps
    link_type: LinkType,                // Ethernet | Serial | Fiber | Tunnel
}

struct SecurityZone {
    id:          Uuid,
    name:        String,                // e.g. "DMZ", "ICU", "GuestWifi"
    role:        ZoneRole,
    trust_level: u8,                    // 0–100
    vlan:        Option<u16>,
    label:       String,                // semantic label e.g. "Clinical-Critical"
}

struct ZonePolicy {
    from_zone: ZoneRef,
    to_zone:   ZoneRef,
    action:    PolicyAction,            // Permit | Deny | Inspect
    services:  Vec<Service>,
    direction: PolicyDirection,         // Unidirectional | Bidirectional
}

struct Interface {
    id:        Uuid,
    name:      String,                  // e.g. "GigabitEthernet0/0"
    ipv4_addr: Option<Ipv4Addr>,
    ipv6_addr: Option<Ipv6Addr>,
    role:      InterfaceRole,           // Uplink | Access | Transit | Loopback | Mgmt
}

struct AddressingPlan {
    ipv4_allocations: Vec<Ipv4Allocation>,
    ipv6_allocations: Vec<Ipv6Allocation>,
}
```

`petgraph` is used for underlying graph traversal (shortest path, neighbor detection, cycle detection). The domain types above wrap it.

### Immutable Graph Versioning

The graph is **never mutated in place**. Every rule execution produces a new graph version. Each version is a complete snapshot:

```rust
struct GraphVersion {
    version:    u32,                    // v1, v2, v3...
    graph:      TopologyGraph,          // full snapshot
    produced_by: String,               // which rule or phase produced this
    timestamp:  DateTime,
    diff:       Vec<GraphChange>,       // what changed from previous version
}

enum GraphChange {
    NodeAdded(NetworkNode),
    NodeRemoved(Uuid),
    EdgeAdded(NetworkLink),
    EdgeRemoved(Uuid),
    ZoneAdded(SecurityZone),
    PolicyAdded(ZonePolicy),
    AddressAssigned { node: Uuid, interface: Uuid, addr: IpAddr },
}
```

The final graph version after all phases and security fixes becomes the truth layer from which all outputs are derived. All prior versions are retained for audit and debugging purposes.

---

## 9. AI Layer — Behavior & Boundaries

### Conversational Design Mode

The AI is not a one-shot intent parser. It is a **persistent design collaborator** — a senior network engineer the user is having a conversation with.

### Conversation State

```rust
struct ConversationState {
    messages:         Vec<Message>,       // full history sent each turn
    current_intent:   PartialIntent,      // accumulates across turns
    resolved_fields:  HashSet<String>,    // what has been decided
    ambiguous_fields: HashSet<String>,    // what needs clarification
    flags:            Vec<DesignFlag>,    // issues AI has noticed
    design_phase:     DesignPhase,        // Gathering | Building | Refining
}

enum DesignPhase {
    Gathering,    // AI asks questions, intent accumulates
    Building,     // intent complete, engine runs, topology appears
    Refining,     // engine has run, user tweaks, engine reruns on delta
}
```

### Phase Transitions

The AI decides when to move from `Gathering` to `Building` — it does not ask the user. When enough fields are resolved to produce a valid intent, the AI transitions, states what it is assuming for unresolved fields, and triggers the engine. The topology starts appearing immediately.

After `Building`, the conversation enters `Refining`. User changes ("make the guest wifi fully isolated") are parsed as delta intents, the graph is updated, and affected outputs regenerate.

### AI Behavior Rules

**Tone:** Senior network engineer. Knowledgeable, direct, never condescending.

**The AI always:**
- Asks one question at a time, never five at once
- Explains why it is asking something
- States assumptions explicitly when inferring
- Flags detected risks immediately, not at the end
- Gives a plain-English summary after building

**The AI never:**
- Asks for information it can reasonably infer from context
- Generates configs in the chat window
- Overrides a user's explicit decision without flagging the risk
- Goes silent during processing

### Example Conversation Flow

```
User:   "I need a network for a hospital"

AI:     "Got it. A few things before I start — how many
         departments need network access? And do you have
         any compliance requirements like HIPAA I should
         design around?"

User:   "ICU, radiology, admin, guest wifi. Yes, HIPAA."

AI:     "Understood. I'll enforce clinical/non-clinical
         separation and isolate medical device traffic.
         Do you need internet access from clinical zones,
         or should those be air-gapped?"

User:   "Internet for radiology only, ICU stays internal"

AI:     "Good call. I'll put radiology behind a DMZ with
         filtered internet access and keep ICU fully
         internal. How many sites — single building or
         multiple locations?"

User:   "Single building, two floors"

AI:     "Perfect. I'll design a collapsed core with
         floor-level distribution. Starting the build now."
         → [engine runs, topology appears in real time]

AI:     "Done. One thing I flagged — your guest wifi and
         radiology DMZ are adjacent zones. I'd recommend
         an explicit deny between them. Want me to add that?"
```

### Hard AI Boundaries

**AI can read:**
- Raw user input
- Generated intent schema (for review)
- Final topology graph (for audit only)
- Security report (for suggestions)
- Decision log (for explanation)

**AI cannot touch:**
- Topology graph directly
- Config output
- Security rule evaluation results
- Addressing plan
- Any engine internal state

**Enforcement:** AI output is always deserialized into a `ParsedIntent` struct via serde. If deserialization or schema validation fails, the output is rejected before entering the pipeline. The AI gets one retry with the validation error appended to the context. If the retry also fails, the user is dropped to the form-based intent builder.

**Fallback:** A structured form-based intent builder is always available. The AI path is a convenience layer, not a load-bearing dependency.

---

## 10. Network Rule Engine

The rule engine is the **compiler of the platform**. It takes the intent schema and produces increasingly refined graph versions through 7 deterministic execution phases. It never thinks — it applies transformations.

### First Principle

The rule engine should never "decide" in an AI sense. Given Intent A, it always produces Graph B — every single time. It is a deterministic transformation engine, not a reasoning system.

### Rule Structure

Every rule has three distinct functions:

```rust
struct EngineRule {
    id:       String,
    name:     String,
    phase:    RulePhase,
    priority: u16,              // higher number = higher priority
    category: RuleCategory,     // Mandatory | Constraint | Preference | Optimization
}

trait Rule {
    // Is this rule applicable to the current intent + graph state?
    fn condition(&self, intent: &NetworkIntent, graph: &TopologyGraph) -> bool;

    // Apply transformation — produces a new graph version, never mutates
    fn transform(&self, graph: &TopologyGraph) -> TopologyGraph;

    // Prove the transformation produced a valid result
    fn validate(&self, graph: &TopologyGraph) -> RuleResult;
}
```

Rules are **isolated** — they never call each other. The engine orchestrates. This makes individual rules independently testable.

### Rule Categories

| Category | Description | Can be overridden? |
|---|---|---|
| Mandatory | Security policies, compliance requirements | Never |
| Constraint | User-specified hard requirements (e.g. Cisco-only) | Only by higher-priority mandatory rule |
| Preference | Soft preferences (e.g. prefer OSPF) | Yes, by constraints |
| Optimization | Nice-to-have improvements | Yes, always lowest priority |

### The 7 Execution Phases

Phases are fixed and sequential. Addressing rules cannot run before topology rules because interfaces don't exist yet.

**Phase 1 — Structural Rules**

Purpose: create the fundamental shape of the network.

```
3 branches + 1 HQ → creates nodes, places devices, establishes basic connectivity
```

Rules: site creation, device placement, basic connectivity, topology type selection (point-to-point / hub-spoke / partial mesh / full mesh).

**Phase 2 — Routing Rules**

Purpose: establish communication paths.

```
OSPF requested → routing domain, areas, neighbor relationships
```

Rules:
```
single org, single AS, <50 routers    → OSPF
multi-AS or ISP peering               → BGP
>50 routers, multiple domains         → OSPF internal + BGP edge
user override                         → respected, flagged if unusual
```

Route summarization, redundancy routing, area design.

**Phase 3 — Segmentation Rules**

Purpose: create trust boundaries.

Rules: VLAN creation, zone creation, DMZ placement, management network injection.

**Phase 4 — Addressing Rules**

Purpose: allocate IP space. Runs after segmentation because zones must exist before subnets are assigned to them.

Rules: subnet generation from host counts, VLAN subnet assignment, WAN link addressing (/30 or /31 IPv4, /127 IPv6), loopback assignment, ULA prefix generation for IPv6.

Default subnet sizes by zone role:

| Zone Role | IPv4 Default | Reason |
|---|---|---|
| LAN | /24 | Standard access layer |
| DMZ | /27 | Limited, controlled hosts |
| MGMT | /28 | Very few devices |
| WAN/Transit | /30 or /31 | Point-to-point links |
| Guest | /23 | High device count |

VLAN allocation: sequential, zone-grouped — LAN 10–99, DMZ 100–199, MGMT 200–299, Transit 300–399.

**Phase 5 — Service Rules**

Purpose: add network services.

Rules: NAT configuration, DHCP scope assignment, DNS server placement, VPN topology.

**Phase 6 — Security Rules**

Purpose: enforce policy at the graph level.

Rules: ACL generation from zone policies, management plane isolation enforcement, OSPF/BGP authentication injection, firewall boundary placement.

**Phase 7 — Optimization Rules**

Purpose: improve design quality. Lowest priority, never overrides other phases.

Rules: route summarization, address aggregation, redundancy tuning, link consolidation.

### Rule Execution Pipeline

```
Phase (1–7)
     ↓
Sort rules by priority (descending)
     ↓
For each rule: evaluate condition()
     ↓
If condition true: apply transform() → new graph version
     ↓
Run validate() — if fails: rule failed, no graph produced, error returned
     ↓
Emit event log entry
     ↓
Next rule
```

### Rule Event Log

Every rule execution emits an event that feeds the audit system and user-facing build timeline:

```rust
struct RuleEvent {
    rule_id:       String,
    phase:         RulePhase,
    action:        String,          // human-readable description
    graph_version: u32,             // which version this produced
    timestamp:     DateTime,
}

// Example
RuleEvent {
    rule_id:       "REQUIRE_DMZ",
    phase:         Segmentation,
    action:        "Created DMZ zone — internet-facing services detected",
    graph_version: 4,
}
```

### Conflict Resolution

Priority hierarchy, highest to lowest:

```
Mandatory Rules (security policies, compliance)
        ↑
Constraint Rules (user hard requirements)
        ↑
Expert Overrides
        ↑
Preference Rules
        ↑
Optimization Rules (defaults)
```

Higher priority always wins. All resolutions are logged with the reason for precedence.

### Rule Packs

Rules are grouped into packs. The engine loads packs based on intent context. This keeps the core engine lean and makes the platform extensible without becoming a monolith.

| Pack | Contents | Loaded when |
|---|---|---|
| Enterprise Pack | HA rules, strict segmentation, compliance rules | environment = production |
| Lab Pack | Simplified designs, reduced security constraints | lab_mode = true |
| Cloud Pack | AWS/Azure rules, hybrid connectivity | cloud provider detected |
| ISP Pack | BGP security, prefix filtering, anti-spoofing | internet_gateway = true AND BGP |
| Compliance Pack | PCI-DSS, HIPAA, NIST, ISO 27001 rule sets | compliance profile detected |

Adding a new pack requires only creating the pack definition and its rules. No changes to the engine itself.

---

## 11. Security Policy DSL

The security policy system is a **network policy compiler**, not a config validator. It expresses what must be true about the network — not how to configure devices. The same policy enforces differently on Cisco, Juniper, and Palo Alto.

### First Principle

```
Good:  USER cannot access MANAGEMENT      ← policy
Bad:   ip access-list extended DENY_MGMT  ← implementation
```

The DSL expresses intent. The vendor adapter implements it.

### Policy Evaluation Flow

```
Final Graph (from rule engine)
        ↓
Policy Engine — evaluates all active policies
        ↓
Violation Report
        ↓
Auto-fix Rules triggered for AutoFix violations
        ↓
New Graph Version produced
        ↓
Re-evaluate — repeat until no violations remain
        ↓
If Critical violations unresolved: export blocked
        ↓
Capability Resolution Engine (next stage)
```

### Policy Structure

```rust
struct SecurityPolicy {
    id:           String,           // e.g. "SP-001"
    name:         String,           // e.g. "Management Isolation"
    category:     PolicyCategory,
    severity:     Severity,         // Critical | High | Medium | Low
    scope:        Vec<Environment>,
    conditions:   Vec<PolicyCondition>,   // when does this policy apply?
    requirements: Vec<PolicyRequirement>, // what must be true?
    remediation:  Option<RemediationRef>, // auto-fix rule to apply
}

enum Severity {
    Critical,   // export blocked until resolved
    High,       // auto-fix attempted
    Medium,     // warning in report
    Low,        // recommendation
}
```

### Policies Separated from Remediations

This is a critical architectural decision. The policy states what must be true. The remediation states how to fix it. They are separate objects.

```rust
struct Remediation {
    id:          String,
    policy_ref:  String,            // which policy this fixes
    transforms:  Vec<GraphTransform>, // what graph changes to apply
}
```

Why: the same policy (`USER cannot reach MANAGEMENT`) may require different graph transforms depending on vendor (ACL on Cisco, zone policy on Palo Alto, firewall filter on Juniper). The policy never changes. The remediation is vendor-aware.

### The 5 Policy Categories

**1. Segmentation Policies**

Define trust boundaries between zones:

```
policy no_user_to_management {
    deny USER -> MANAGEMENT
}

policy dmz_isolation {
    deny DMZ -> LAN
    deny DMZ -> MANAGEMENT
}

policy guest_isolation {
    deny GUEST -> any internal zone
    permit GUEST -> INTERNET
}
```

**2. Routing Policies**

Protocol-level security requirements, vendor-agnostic:

```
policy secure_ospf {
    require ospf.authentication = true
}

policy secure_bgp {
    require bgp.prefix_filtering = true
    require bgp.max_prefixes = defined
}
```

**3. Management Plane Policies**

```
policy isolated_management {
    require management.zone = exists
    deny USER -> MANAGEMENT
    deny GUEST -> MANAGEMENT
    deny DMZ -> MANAGEMENT
}

policy management_access {
    permit VPN -> MANAGEMENT
    permit MGMT_JUMP_HOST -> all zones
}
```

**4. Service Policies**

```
policy internet_exposed_services {
    require DMZ = exists
    require internet_gateway behind DMZ
}

policy internal_dns_only {
    deny internal zones -> external DNS
    require internal DNS server
}
```

**5. Compliance Policies**

```
policy pci_dss {
    require segmentation = true
    require logging = true
    require DMZ = true
    require cardholder_zone isolation
}

policy zero_trust {
    deny any -> any (default)
    require explicit_permit for every flow
}

policy hipaa {
    require clinical_nonclinical_separation
    require iomt_zone isolation
    require audit_logging on clinical adjacent firewalls
}
```

### Severity Levels

| Severity | Behavior | Example |
|---|---|---|
| Critical | Export blocked. User must fix or enable lab mode. | USER can reach MANAGEMENT |
| High | Auto-fix attempted. Fix logged. User notified. | Missing DMZ for internet gateway |
| Medium | Warning in security report. Not blocking. | Guest VLAN adjacent to DMZ |
| Low | Recommendation. Lowest visibility. | Route summarization opportunity |

### Security Profiles

Each security level in the intent schema loads a different policy pack:

**Basic** — SSH only, no Telnet, no SNMPv1/v2

**Standard** — Basic + management isolation, DMZ enforcement, logging

**Hardened** — Standard + zero trust segmentation, full routing authentication, strict ACL deny-by-default, complete audit logging

### Policy Packs

| Pack | Contents | Loaded when |
|---|---|---|
| Base Pack | SSH-only, no Telnet, SNMPv3, ACL deny-by-default | Always |
| Enterprise Pack | Management isolation, DMZ enforcement, HA requirements | environment = production |
| ISP Pack | BGP prefix filtering, anti-spoofing, route policy | BGP + internet_gateway |
| Cloud Pack | VPC isolation, transit security, peering restrictions | Cloud provider in vendor list |
| HIPAA Pack | Clinical separation, IoMT isolation, audit logging | HIPAA compliance detected |
| PCI-DSS Pack | Cardholder isolation, segmentation, logging | PCI-DSS compliance detected |
| Zero Trust Pack | Default-deny everything, explicit permits only | security_level = hardened |

### Policy Traceability

Every policy action is recorded. Every ACL, every zone boundary, every firewall rule must be traceable to the policy that required it:

```rust
struct PolicyEvent {
    policy_id:     String,          // "SP-001"
    action:        String,          // "created management VLAN isolation ACL"
    graph_version: u32,
    timestamp:     DateTime,
}
```

Example query from an engineer: *"Why does this ACL exist?"*
Answer: `SP-001 — Management Isolation policy requires USER zones cannot reach MANAGEMENT zone.`

Not: *"The system decided to."* Engineers reject unexplained automation.

---

## 12. Vendor Capability Model

### First Principle

The system thinks in **capabilities**, never vendors. The graph and rule engine have no knowledge of Cisco, Juniper, or MikroTik. They know only: routing, firewalling, NAT, VPN, ACL, VRF, high availability.

Vendor selection happens late in the pipeline — after the graph is fully designed and verified.

```
Bad:  Use Cisco OSPF
Good: Dynamic Routing Capability
```

This means a network can be fully designed, security-verified, and validated before a single vendor is involved.

### Capability Model

Every vendor exposes a capability map:

```rust
struct VendorProfile {
    vendor:       Vendor,
    family:       String,           // e.g. "ios_xe", "junos", "routeros"
    version:      String,           // e.g. "17.x", "23.x"
    capabilities: Vec<Capability>,
}

struct Capability {
    id:           String,           // e.g. "ospf", "bgp", "dmvpn"
    category:     CapabilityCategory,
    level:        CapabilityLevel,  // Basic | Standard | Advanced | Enterprise
    limitations:  Vec<String>,      // known constraints
}

enum CapabilityLevel {
    Basic,       // feature exists, minimal options
    Standard,    // full feature set
    Advanced,    // advanced options (authentication, stub areas, etc.)
    Enterprise,  // full enterprise feature set
}
```

Support is **not binary**. A vendor that supports OSPF but not OSPFv3 authentication is not the same as one that supports both. The level matters.

### Capability Categories

| Category | Examples |
|---|---|
| Routing | OSPF, BGP, Static, IS-IS, Route Summarization, ECMP |
| Security | ACL, Zone Firewall, IDS/IPS |
| Segmentation | VLAN, VRF, VXLAN, Private VLAN |
| WAN | IPSec, GRE, DMVPN, SD-WAN |
| Services | DHCP, DNS, NAT, Load Balancing |
| High Availability | HSRP, VRRP, GLBP, Link Redundancy |

### Vendor Profiles (Phased)

**Phase 1 — Cisco IOS / IOS-XE**
```rust
VendorProfile {
    vendor:  Cisco,
    family:  "ios_xe",
    version: "17.x",
    capabilities: [
        Capability { id: "ospf",    level: Enterprise },
        Capability { id: "bgp",     level: Enterprise },
        Capability { id: "nat",     level: Advanced   },
        Capability { id: "acl",     level: Advanced   },
        Capability { id: "dmvpn",   level: Advanced   },
        Capability { id: "vrf",     level: Advanced   },
        Capability { id: "hsrp",    level: Advanced   },
    ]
}
```

**Phase 2 — Juniper JunOS**

**Phase 3 — MikroTik RouterOS**

**Phase 4 — Palo Alto (firewall capabilities only)**

### Capability Resolution Engine

A dedicated pipeline stage that runs after the security policy engine and before config generation.

```
Final Graph (capabilities required per node)
        ↓
Capability Resolution Engine
        ↓
For each node: check vendor_profile.capabilities covers graph requirements
        ↓
If gap found: CapabilityError { node, missing_capability, vendor }
        ↓
Generation blocked — user must change vendor or remove requirement
```

Example:
```
Graph requires: OSPF + DMVPN + VRF
Vendor selected: MikroTik RouterOS

OSPF   ✔
DMVPN  ✗
VRF    ✗

Result: Generation blocked
        MikroTik RouterOS does not support DMVPN or VRF.
        Suggested alternative: Cisco IOS-XE
```

### Capability Contracts

The security engine can declare capability contracts — requirements that must be satisfiable before generation proceeds:

```rust
struct CapabilityContract {
    feature:  String,
    required: bool,
    reason:   String,   // which security policy requires this
}

// Example: OSPF authentication is a security policy requirement
CapabilityContract {
    feature:  "ospf_authentication",
    required: true,
    reason:   "SP-010 secure_ospf policy",
}
```

If the selected vendor cannot satisfy a capability contract, generation is blocked before any config is written.

### Multi-Vendor Networks

Each node carries its own vendor profile. The system supports heterogeneous networks natively:

```
RTR-CORE-01    → vendor: cisco_ios_xe
SW-DIST-01     → vendor: cisco_ios_xe
FW-DMZ-01      → vendor: palo_alto
RTR-BRANCH-01  → vendor: juniper_junos
```

The capability engine validates each node independently. The config generator calls each node's vendor adapter independently.

### Vendor Adapters

Adapters translate the generic graph into vendor-specific config syntax. They implement the `VendorAdapter` trait:

```rust
trait VendorAdapter {
    fn render_interface(&self, iface: &Interface) -> String;
    fn render_routing(&self, routing: &RoutingConfig) -> String;
    fn render_acl(&self, policy: &ZonePolicy) -> String;
    fn render_hardening(&self, level: SecurityLevel) -> String;
    fn render_vlan(&self, vlan: &VlanConfig) -> String;
    fn interface_name(&self, role: InterfaceRole, index: u8) -> String;
    fn appliance_id(&self) -> &str;     // GNS3 appliance template ID
}
```

Every generated config object carries a traceability record:

```rust
struct ConfigObject {
    content:      String,
    generated_by: String,       // which adapter
    capability:   String,       // which capability this implements
    graph_object: Uuid,         // which graph node/edge this came from
    policy_ref:   Option<String>, // which security policy required it
}
```

### Adding a New Vendor

Adding a vendor requires only three things — nothing else in the system changes:

1. Create `VendorProfile` with capability map
2. Create `VendorAdapter` implementation
3. Write template files for that vendor

The rule engine, security engine, graph model, and capability resolution engine are completely unchanged.

### Phased Rollout

| Phase | Vendor | Scope |
|---|---|---|
| 1 (MVP) | Cisco IOS / IOS-XE | Routers + L3 switches |
| 2 | Juniper JunOS | Routers + switches |
| 3 | MikroTik RouterOS | Routers (prosumer/lab market) |
| 4 | Palo Alto | Firewall configs only |

---

## 13. Config Generation

Configs are **never generated by AI**. They are produced exclusively by deterministic templates populated with data from the topology graph.

### Template Engine

`minijinja` (Rust port of Jinja2) handles all config rendering. One template set per vendor, one template per config section.

### Template Structure

```
netgen-core/src/templates/
  cisco/
    ios/
      base.j2           # hostname, domain, banner
      interfaces.j2     # interface config with IPs
      ospf.j2           # OSPF process config
      bgp.j2            # BGP neighbor config
      acl.j2            # access control lists
      hardening.j2      # SSH, SNMPv3, no telnet, service password-encryption
      vlan.j2           # VLAN database + SVI config
  juniper/
    junos/
      ...               # same structure
```

### Config Contents (per device)

Every generated config includes, at minimum:
- Hostname + domain name
- Interface configs with IPv4 and/or IPv6 addressing
- Routing protocol config (correct area/AS assignments, authentication)
- ACLs derived from zone policies
- SSH-only access (no Telnet — always)
- SNMPv3 (when management enabled)
- Login banner
- Security hardening baseline per security level
- NTP sync (when NTP service enabled)

### Config Viewer

Configs are viewable per-device in the UI with syntax highlighting. Downloadable as individual `.cfg` files or a full ZIP archive.

---

## 14. IPv4 & IPv6 Addressing Engine

### Three Modes

| Mode | Description |
|---|---|
| `Ipv4Only` | Classic — most labs and legacy enterprise |
| `DualStack` | Both protocols simultaneously — modern enterprise |
| `Ipv6Only` | Greenfield — rare today, supported for future-proofing |

### Dual-Stack Implications

In dual-stack mode, every interface gets both an IPv4 and IPv6 address. Routing runs on both planes:
- IPv4: OSPFv2 or BGP-4
- IPv6: OSPFv3 or BGP-4+ (multiprotocol extensions)

ACLs are generated separately for each protocol plane. Templates produce both sets per vendor.

### IPv6 Specifics

| Component | IPv4 | IPv6 |
|---|---|---|
| Link addresses | /30 or /31 per link | /127 per link (RFC 6164) |
| Internal ranges | RFC 1918 | ULA fc00::/7 |
| External | Public IP | GUA (if provided) |
| NAT | PAT/NAT common | NAT64 only when translating; otherwise no NAT |
| ACLs | Standard IPv4 | Separate IPv6 ACL syntax per vendor |

### ULA Auto-Generation

If no ULA prefix is specified in the intent, the addressing engine generates one automatically per RFC 4193 (random global ID). This is logged in the decision log as an automatic action.

### Allocation Strategy

Subnets are calculated from declared host counts per zone. If no count is given, defaults apply by zone role:

| Zone Role | Default IPv4 Subnet | Reason |
|---|---|---|
| LAN | /24 | Standard access layer |
| DMZ | /27 | Limited hosts, controlled |
| MGMT | /28 | Very few devices |
| WAN/Transit | /30 or /31 | Point-to-point links |
| Guest | /23 | High device count expected |

---

## 15. Intelligent Labelling System

### Responsibility Split

| Task | Owner |
|---|---|
| Understanding "ICU" means clinical-critical | AI |
| Inferring guest wifi = untrusted zone | AI |
| Detecting industry context from keywords | AI |
| Applying naming convention format | Code (deterministic) |
| Assigning VLAN numbers sequentially | Code (deterministic) |
| Suggesting MGMT zone if not mentioned | Rule engine |
| Generating semantic zone labels | AI |

### Device Naming Convention

```
{DEVICE_TYPE}-{ZONE/FUNCTION}-{ROLE}-{INDEX}

Device type codes:
  RTR  = router
  SW   = switch
  FW   = firewall
  AP   = access point
  SRV  = server

Examples:
  RTR-CORE-01
  RTR-WAN-01
  SW-ICU-ACC-01
  SW-CORE-DIST-01
  FW-DMZ-01
  AP-GUEST-01
```

Code enforces the format. AI provides the semantic zone name. The naming engine combines them deterministically.

### Zone Semantic Labels

From the intent context, AI produces semantic labels that appear in diagrams, reports, and documentation:

```
"ICU"       → label: "Clinical-Critical"
"Radiology" → label: "Clinical-Imaging"
"Admin"     → label: "Corporate"
"GuestWifi" → label: "Untrusted-Access"
"MGMT"      → label: "OOB-Management"    (always injected by rule engine)
```

### Industry Context Awareness

When industry context is detected, the AI pre-populates zone suggestions and the security engine loads the corresponding compliance profile:

| Industry | Auto-detected zones | Compliance profile loaded |
|---|---|---|
| Healthcare | Clinical, Non-clinical, IoMT, Admin, Guest | HIPAA |
| Retail | POS, Back-office, Guest | PCI-DSS |
| Education | Student, Staff, Admin, Guest | None (suggested) |
| Industrial/OT | IT, OT, SCADA, Historian, DMZ | IEC 62443 (suggested) |
| Enterprise | LAN, DMZ, MGMT, Guest | ISO 27001 (suggested) |

---

## 16. Output Specification

All outputs derive from the single topology graph. Every output regenerates consistently when the graph changes.

### Output Matrix

| Output | Format | Delivery |
|---|---|---|
| Topology canvas | Interactive React Flow graph | Live in UI |
| Architecture diagram | Interactive SVG + PNG export | UI viewer + download |
| IP addressing plan | Structured table | UI + CSV download |
| Device configs | Per-device .cfg files | UI viewer + ZIP download |
| Security report | Structured report | UI + PDF/Markdown download |
| GNS3 project export | .gns3project ZIP | Download |
| Simulation view | Browser-based interactive | Live in UI |
| Decision log | Always-present audit trail | UI + included in all doc exports |
| AI audit report | On-demand | UI + PDF download |

### IP Plan Structure

```
Site / Zone / Device / Interface / IPv4 Address / IPv6 Address / Subnet / Purpose

Site-1 / LAN-ICU    / SW-ICU-ACC-01 / Vlan10  / 10.10.10.1/24 / fd00::10:1/64 / ICU access layer
Site-1 / DMZ        / FW-DMZ-01     / e0/1    / 10.10.20.1/27 / fd00::20:1/64 / DMZ gateway
Site-1 / MGMT       / RTR-CORE-01   / Vlan99  / 10.10.99.1/28 / fd00::99:1/64 / OOB management
```

### Security Report Structure

```
Security Report — [Network Name]
Generated: [timestamp]

Security Level:    [Basic | Standard | Hardened]
Environment:       [Lab | Staging | Production]
Compliance Context: [detected profiles]

Zone Policy Matrix:
  [from_zone] → [to_zone] : [PERMIT | DENY | INSPECT]
  ...

Rules Triggered:
  ✓ [rule_id]    [action taken]
  ⚠ [rule_id]    [warning, user-acknowledged]
  ✗ [rule_id]    [blocked — not shown if auto-fixed]

AI Audit (if requested):
  [risk flags, suggestions, observations]
```

---

## 17. Interactive Features

### Layer 1 — Interactive Topology Canvas

The React Flow topology is a full design canvas, not a read-only diagram.

**Click interactions:**
- Click node → side panel: device details, interfaces, IPs, config preview
- Click link → shows bandwidth, link type, interface names, addresses on both ends
- Click zone → highlights all devices in zone, shows zone policy summary
- Right-click node → context menu: "View config", "Rename", "Change vendor", "Delete", "Add link"
- Right-click zone → "Add device", "Edit zone policy", "View security rules"

**Edit interactions:**
- Drag node → repositions it, saved with project
- Add/remove links visually
- Rename devices inline

### Layer 2 — Interactive Architecture Diagram

The architecture diagram (professional "looks like Packet Tracer" aesthetic) is not just a PNG — it is an interactive SVG rendered in the UI.

**Visual style:**
- Proper network iconography (Cisco-style router symbol, switch symbol, firewall shield, cloud for internet)
- Right-angle (orthogonal) link routing for professional appearance
- Zone boundaries as labeled soft rounded rectangles
- Link labels showing interface names and addresses
- Title block with network name, date, version
- Legend in corner

**Interactions:**
- Hover device → tooltip: hostname, vendor, role, IP summary
- Hover link → interface names and addresses on both ends
- Click device → opens config viewer for that device
- Click zone boundary → zone policy matrix for that zone
- Toggle layers → show/hide: IP labels, interface labels, zone boundaries, link types
- Export → "Export as PNG" captures current view exactly as shown

**Export formats:** SVG (scalable, clean at any size) + PNG (for presentations and documentation)

### Layer 3 — Browser-Based Simulation View

A simulation view that approximates the GNS3 experience in the browser, without requiring GNS3. Does not simulate actual packet forwarding — simulates the device state faithful to the generated configs.

**Device status panel:**
```
[Start Lab] → devices show "booting" → status turns green "running"
              → links show "active" with animated flow indicators
```

**Per-device CLI view:**
Click any device → opens a panel showing a faithful CLI representation:
```
RTR-CORE-01#show ip interface brief

Interface          IP-Address     OK?  Method  Status    Protocol
GigabitEthernet0/0 10.0.0.1      YES  NVRAM   up        up
GigabitEthernet0/1 10.10.0.1     YES  NVRAM   up        up
Vlan99             10.10.99.1    YES  NVRAM   up        up
```

Generated deterministically from the topology graph — faithful representation of real device output.

**Route table view:**
```
RTR-CORE-01#show ip route

O    10.10.20.0/27 [110/2] via 10.0.0.2, GigabitEthernet0/0
O    10.10.30.0/24 [110/3] via 10.0.0.2, GigabitEthernet0/0
C    10.10.99.0/28 is directly connected, Vlan99
```

Calculated by the route calculator from routing protocol config and topology structure.

**Path tracer:**
Select source device + destination device → system traces the path:
```
ICU-PC-01 → Radiology-Server-01

Hop 1:  SW-ICU-ACC-01    [VLAN 10 → uplink]
Hop 2:  RTR-CORE-01      [route lookup → 10.10.20.0/27]
Hop 3:  FW-DMZ-01        [ACL check → DENY]

Result: ✗ BLOCKED
Reason: Zone policy — ICU → DMZ = DENY (Clinical isolation, HIPAA profile)
```

**Simulation engine architecture:**
```
TopologyGraph
      ↓
RouteCalculator     → computes routing tables per device (deterministic)
      ↓
ACLEvaluator        → evaluates zone policies per flow
      ↓
PathTracer          → walks hops from source to destination
      ↓
CLIRenderer         → formats output as realistic CLI text
```

All Rust. All deterministic. The frontend renders what the engine calculates.

---

## 18. Lab Export Strategy

### GNS3 Export (Primary)

Full GNS3 project structure, ready to open immediately:

```
[network-name].gns3project/
  project.gns3              # manifest — node/link definitions, appliance mappings
  project-files/
    configs/
      RTR-CORE-01.cfg
      SW-ICU-ACC-01.cfg
      FW-DMZ-01.cfg
      ...
    topology/
      nodes.json            # positions, appliance template IDs, interface assignments
      links.json            # interface connections between nodes
```

**Node mapping:** Each graph node maps to a GNS3 appliance template. The mapping table is configurable per installation so users can point to their own images.

**Interface naming:** Assigned deterministically by device type and port count:
- Cisco: `GigabitEthernet0/0`, `GigabitEthernet0/1`, ...
- Juniper: `ge-0/0/0`, `ge-0/0/1`, ...

**Coordinate layout:** Calculated using a layered layout algorithm — WAN at top, core in middle, access/DMZ zones lower. Zones are visually separated. No manual positioning required.

**User experience:** Download ZIP → open GNS3 → File → Import portable project → network is wired, configs are loaded, start devices and run.

### Packet Tracer

Explicitly out of scope. The `.pkt` format is proprietary, undocumented, and changes between versions without notice. Supporting it would create an unmaintainable dependency.

### Additional Exports

| Export | Format | Contents |
|---|---|---|
| Architecture diagram | SVG + PNG | Professional static diagram |
| IP addressing plan | CSV | Full address table |
| Network documentation | Markdown + PDF | Full design doc with decisions |
| Config archive | ZIP of .cfg files | All device configs |
| Security report | PDF | Security policy evaluation results |

---

## 19. UI/UX & Product Experience

### Interaction Model

**Chat-first, form as fallback, always live preview.**

```
┌────────────────────────────────────────────────────────┐
│  [Chat Input]  "3-site OSPF hospital network, HIPAA"   │
├────────────────────────┬───────────────────────────────┤
│                        │                               │
│   Live Topology        │   Build Timeline              │
│   (React Flow)         │   ① Intent parsed             │
│                        │   ② Zones assigned            │
│   [nodes + edges       │   ③ Addressing calculated     │
│    appear as engine    │   ④ Rules evaluated           │
│    runs, zone by zone] │   ⑤ Security checked          │
│                        │   ⑥ Configs generated         │
│                        │   ⑦ Simulation ready          │
├────────────────────────┴───────────────────────────────┤
│  [Topology] [Diagram] [Configs] [IP Plan] [Security]   │
│  [Simulation]  [Export GNS3]  [Export Docs]  [Audit]   │
└────────────────────────────────────────────────────────┘
```

### Streaming

AI responses stream in real time. As the engine runs, the topology graph updates progressively:

```
AI signals "building"
  → zones appear
  → nodes appear within zones
  → links connect nodes
  → IP addresses fill in on interfaces
  → config generation completes
  → simulation view becomes available
  → AI posts plain-English summary
```

### "Why this?" Feature

Every design element — every node, link, zone, ACL, subnet — has a "Why this?" button. Clicking it opens the decision log entry for that element, explaining:
- What triggered the decision (user intent, rule engine, security policy, AI inference)
- Which specific rule fired
- Whether the decision is overridable

This is the single most important trust-building feature in the product.

### Key UI Panels

| Panel | Contents |
|---|---|
| Chat | Conversation history, AI responses, input field |
| Topology canvas | Interactive React Flow graph, real-time updates |
| Architecture diagram | SVG diagram with layer toggles |
| Config viewer | Syntax-highlighted per-device configs |
| IP Plan | Sortable/filterable address table |
| Security report | Policy matrix, rule results, compliance observations |
| Simulation | Device status, CLI view, path tracer |
| Decision log | Full audit trail, filterable by component/rule/trigger |
| AI audit | On-demand audit report with risk flags |

---

## 20. AI Audit System

### Optional, User-Triggered

The AI audit is not mandatory. The user explicitly requests it. It does not gate export in any mode.

### What the Audit Does

The AI reads:
- The full topology graph
- The decision log
- The generated configs
- The security report

It returns:
- **Risk flags** — potential issues not caught by security rules (e.g. "this ACL has an implicit permit that may be unintentional")
- **Design suggestions** — improvements worth considering (e.g. "consider route summarization at area boundary")
- **Compliance observations** — notes relevant to detected industry context
- **Plain-English design summary** — suitable for including in documentation or presenting to a team

### What the Audit Does NOT Do

- Does not modify the graph
- Does not modify any output
- Does not block or gate export
- Does not override engine decisions
- Does not rewrite security rule results

### Audit Report

The AI audit report is a **separate output artifact**. It is never mixed into the main decision log. The distinction is explicit:
- Decision log = "what the system decided and why" (always present, engine-generated)
- Audit report = "what the AI observed" (on request, read-only analysis)

---

## 21. Trust & Auditability

Every output ships with a decision log. This is what makes engineers trust the system.

### Decision Log Entry Format

```rust
struct DesignDecision {
    id:           Uuid,
    component:    String,           // "DMZ zone", "OSPF area 0", "ACL on FW-DMZ-01"
    reason:       String,           // human-readable plain English
    rule_id:      Option<String>,   // which rule triggered this, if any
    triggered_by: TriggerSource,    // UserIntent | RuleEngine | SecurityPolicy | AiInference
    overridable:  bool,             // can the user override this?
    timestamp:    DateTime,
}
```

### Example Decision Log Entries

```
component:    "DMZ zone added"
reason:       "internet_gateway=true detected in intent. Production security
               level requires traffic isolation for internet-facing services."
rule_id:      "DMZ_REQUIRED_FOR_INTERNET"
triggered_by: RuleEngine
overridable:  false  (production mode)

component:    "OSPFv3 enabled on all interfaces"
reason:       "dual-stack addressing mode selected. OSPFv3 required for
               IPv6 routing plane alongside OSPFv2 for IPv4."
rule_id:      null
triggered_by: RuleEngine
overridable:  true

component:    "Guest VLAN assigned VLAN 40"
reason:       "sequential VLAN allocation, zone role = Guest.
               LAN range: 10–39 used. Next available: 40."
rule_id:      null
triggered_by: RuleEngine
overridable:  true

component:    "Zone label 'Clinical-Critical' assigned to ICU"
reason:       "AI inferred clinical-critical classification from department
               name 'ICU' in healthcare context."
rule_id:      null
triggered_by: AiInference
overridable:  true
```

### Log Availability

The decision log is always present, always exported alongside configs and diagrams. It cannot be disabled. Engineers can filter it by component, rule, or trigger source.

---

## 22. Failure Handling Strategy

**Principle: no partial outputs. Ever.**

| Failure Point | Response |
|---|---|
| AI produces invalid intent | Validate → reject → retry once with validation error appended → if retry fails, drop to form-based input |
| Rule engine conflict | Priority system resolves (security > topology > user preference). Resolution is logged. |
| Security engine blocks | Export refused. Specific rule ID + message returned. User must fix or enable lab mode. |
| Config generation fails | Full generation rolled back. No partial configs exported. Error identifies device + template. |
| Conflicting requirements | Engine surfaces conflict explicitly ("redundancy=high conflicts with single uplink at site-2"), offers resolution options. |
| AI service unavailable | Form-based intent builder activates automatically. No degradation to core functionality. |
| Addressing exhaustion | Engine surfaces specific conflict (which zone ran out of space), suggests base range adjustment. |

### Error Response Format

All pipeline errors return a structured error, never a generic message:

```rust
struct PipelineError {
    stage:      PipelineStage,      // which stage failed
    rule_id:    Option<String>,     // if a rule caused it
    component:  Option<String>,     // which component is involved
    message:    String,             // human-readable explanation
    resolution: Vec<String>,        // suggested fixes
    blocking:   bool,               // does this prevent export?
}
```

---

## 23. Versioning System

### Versioned Components

| Component | Version scheme |
|---|---|
| Intent schema | `intent_schema_v1`, `intent_schema_v2`, ... |
| Rule engine | `rule_engine_v1`, `rule_engine_v2`, ... |
| Vendor templates | `cisco_ios_v1`, `juniper_v1`, ... |
| Security policies | `security_policy_v1`, `security_policy_v2`, ... |

### Per-Project Version Snapshot

Every saved project stores:

```json
{
  "schema_version":    "intent_schema_v1",
  "rules_version":     "rule_engine_v1",
  "template_versions": { "cisco": "cisco_ios_v1" },
  "policy_version":    "security_policy_v1",
  "generated_at":      "2026-06-06T00:00:00Z"
}
```

### Migration Strategy

When a schema version increments:
- A migration function converts old projects forward automatically
- Breaking changes get a major version bump + explicit migration tooling
- Old projects never silently break — they either migrate cleanly or surface a migration error with instructions

---

## 24. Scale Strategy

### Target: SaaS with self-hosted option

| Segment | Deployment | Model |
|---|---|---|
| Individuals, students, small teams | Cloud SaaS | Free tier + paid plans |
| Security-conscious orgs, enterprise | Self-hosted | Paid license, single binary |

Both targets share one codebase. The Rust backend compiles to a single binary for self-hosting — no dependency installation, no container required (though Docker is supported).

### Architecture for Scale

- **Stateless API** — each request is independent, scales horizontally
- **Async job queue** — network generation jobs are heavy, handled off the main thread via apalis
- **Caching** — deterministic designs with identical intents can cache results
- **Per-user project storage** — PostgreSQL, row-level security per user
- **Rate limiting on AI calls** — per tier, enforced at middleware level
- **No vendor lock-in** — runs on any VPS, not tied to AWS-specific services

### Not in v1

Plugin ecosystem — this is a v3 concern. Building it too early adds maintenance burden before product-market fit is established.

---

## 25. Build Phases

### Phase 1 — Intent Schema

Define and validate the complete intent schema as Rust structs with serde. Write schema validation tests with both valid and invalid intents. This is the contract everything else is built on.

**Deliverable:** `netgen-core/src/schema/` — fully typed, validated, versioned intent structs.

### Phase 2 — Topology Graph + Versioning

Implement the topology graph data model wrapping petgraph. Define all node, edge, zone, policy, and interface types. Implement immutable graph versioning — every transformation produces a new version, never mutates in place.

**Deliverable:** `netgen-core/src/graph/` — topology graph with petgraph backend and full version history.

### Phase 3 — Rule Engine (7 Phases)

Implement the rule engine framework: phase orchestration, rule trait (condition / transform / validate), priority sorting, event log emission. Implement all 7 phases starting with structural rules. Each phase tested independently with hardcoded intents — no AI yet. Implement Enterprise Pack and Lab Pack.

**Deliverable:** `netgen-core/src/rule_engine/` — full 7-phase engine producing versioned graph from intent.

### Phase 4 — Security Policy Engine + DSL

Implement the policy evaluator, all 5 policy categories, separation of policies from remediations, severity levels, and auto-fix loop. Implement Base Pack + Enterprise Pack policies. Lab mode. Test blocking, auto-fix, and warn behaviors.

**Deliverable:** `netgen-core/src/security/` — policy DSL engine with packs and remediation system.

### Phase 5 — Vendor Capability Model

Implement VendorProfile, Capability structs with levels, CapabilityResolutionEngine, CapabilityContracts. Implement Cisco IOS-XE profile with full capability map.

**Deliverable:** `netgen-core/src/capability/` — capability resolution engine + Cisco profile.

### Phase 6 — Addressing Engine

Implement IPv4 and IPv6 allocation. Subnet sizing from zone roles, VLAN assignment, link addressing, ULA auto-generation per RFC 4193.

**Deliverable:** `netgen-core/src/addressing/` — deterministic addressing plan from topology graph.

### Phase 7 — Config Templates + Cisco Adapter

Implement minijinja templates for Cisco IOS-XE. Interfaces, OSPF, BGP, ACLs, VLANs, hardening baseline. Implement CiscoAdapter. Test against known-good Cisco configs.

**Deliverable:** `netgen-core/src/templates/cisco/` + `netgen-core/src/adapters/cisco.rs`

### Phase 8 — Verification Layer

Post-generation automated tests: interface consistency, routing neighbor reachability, zone policy completeness, addressing conflicts, capability contract satisfaction.

**Deliverable:** `netgen-core/src/verification/`

### Phase 9 — Simulation Engine

Route calculator, ACL evaluator, path tracer, CLI renderer. All deterministic from the final graph.

**Deliverable:** `netgen-core/src/simulation/`

### Phase 10 — Export Layer

GNS3 project generator, architecture diagram SVG renderer, IP plan CSV exporter, documentation generator.

**Deliverable:** `netgen-core/src/export/`

### Phase 11 — AI Conversation Layer

Integrate Anthropic API. Implement conversation state management, phase transitions (Gathering → Building → Refining), schema validation of AI output, retry logic, fallback to form input. Implement intelligent labelling prompt and industry context detection.

**Deliverable:** `netgen-api/src/` — AI conversation endpoint with full pipeline integration.

### Phase 12 — Axum API Server

REST + WebSocket API. Auth, rate limiting, async job queue, PostgreSQL integration, user project storage.

**Deliverable:** `netgen-api/` — production API server.

### Phase 13 — Next.js Frontend

Chat interface, React Flow topology canvas, architecture diagram viewer, config viewer, simulation UI, all output panels, export buttons, "Why this?" decision log integration.

**Deliverable:** `netgen-web/` — full frontend application.

### Phase 14 — Tauri Desktop App

Wrap the frontend in Tauri v2. Wire to netgen-core directly for offline use. Package for Linux, macOS, Windows.

**Deliverable:** `netgen-desktop/` — installable desktop application.

### Phase 15 — Juniper Adapter

Add Juniper JunOS VendorProfile, capability map, JuniperAdapter, and template set. Template parity with Cisco.

### Phase 16 — SaaS Launch

Billing integration, pricing tiers, onboarding flow, public launch on labyrinth.network.

---

*Plan version: 1.1 — June 2026*
*Author: Adeyinka Adejumo (AdeyLord)*
*GitHub: github.com/Mosope-ade/labyrinth*
