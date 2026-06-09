// TypeScript types mirroring the Rust API response shapes.

// ─── AI / Chat ────────────────────────────────────────────────────────────────

export type DesignPhase = 'gathering' | 'building' | 'refining'

export type Industry =
  | 'healthcare'
  | 'retail'
  | 'education'
  | 'industrial'
  | 'enterprise'

export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
}

export interface ChatTurnResponse {
  session_id: string
  phase: DesignPhase
  reply: string
  intent_ready: boolean
  fallback_required: boolean
  detected_industry: Industry | null
  project_id: string | null
}

// ─── Auth ─────────────────────────────────────────────────────────────────────

export interface AuthResponse {
  user_id: string
  email: string
  token: string
}

export interface LoginResponse extends AuthResponse {
  tier: string
}

// ─── Projects ────────────────────────────────────────────────────────────────

export type ProjectStatus = 'draft' | 'generating' | 'ready' | 'error'

export interface Project {
  id: string
  name: string
  status: ProjectStatus
  created_at: string
  updated_at: string
}

export interface ProjectDetail extends Project {
  intent_json: unknown
  graph_json: TopologyGraph | null
}

export interface ConversationTurn {
  id: string
  role: 'user' | 'assistant'
  content: string
  created_at: string
}

// ─── Topology Graph (mirrors netgen-core TopologyGraph) ──────────────────────

export type NodeRole = 'router' | 'switch' | 'firewall' | 'host' | 'cloud' | 'ap'
export type ZoneRole = 'lan' | 'wan' | 'dmz' | 'mgmt' | 'transit' | 'oob' | 'guest'
export type PolicyAction = 'permit' | 'deny' | 'inspect'
export type LinkType = 'ethernet' | 'serial' | 'fiber' | 'tunnel'
export type TriggerSource = 'user_intent' | 'rule_engine' | 'security_policy' | 'ai_inference'

export interface Interface {
  id: string
  name: string
  ipv4_addr: string | null
  ipv6_addr: string | null
  role: string
}

export interface NetworkNode {
  id: string
  label: string
  role: NodeRole
  vendor: string
  zone_id: string
  interfaces: Interface[]
  config: DeviceConfig | null
}

export interface NetworkLink {
  id: string
  from_node: string
  from_iface: string
  to_node: string
  to_iface: string
  bandwidth: number | null
  link_type: LinkType
}

export interface SecurityZone {
  id: string
  name: string
  role: ZoneRole
  trust_level: number
  vlan: number | null
  label: string
}

export interface ZonePolicy {
  from_zone: string
  to_zone: string
  action: PolicyAction
  services: string[]
  direction: string
}

export interface Ipv4Allocation {
  node_id: string
  interface_id: string
  address: string
  prefix_len: number
  zone_id: string
}

export interface Ipv6Allocation {
  node_id: string
  interface_id: string
  address: string
  prefix_len: number
  zone_id: string
}

export interface AddressingPlan {
  ipv4_allocations: Ipv4Allocation[]
  ipv6_allocations: Ipv6Allocation[]
}

export interface DesignDecision {
  id: string
  component: string
  reason: string
  rule_id: string | null
  triggered_by: TriggerSource
  overridable: boolean
  timestamp: string
}

export interface DeviceConfig {
  raw: string
  template_version: string
}

export interface TopologyGraph {
  nodes: NetworkNode[]
  edges: NetworkLink[]
  zones: SecurityZone[]
  policies: ZonePolicy[]
  addressing: AddressingPlan
  decisions: DesignDecision[]
  metadata: {
    name: string
    version: number
    created_at: string
    updated_at: string
  }
}

// ─── Job events (WebSocket) ────────────────────────────────────────────────

export type JobEvent =
  | { type: 'started'; job_id: string }
  | { type: 'progress'; job_id: string; step: string; percent: number }
  | { type: 'complete'; job_id: string }
  | { type: 'failed'; job_id: string; error: string }
