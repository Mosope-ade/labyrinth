'use client'

import { useCallback, useMemo } from 'react'
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  type Node,
  type Edge,
  type NodeMouseHandler,
  BackgroundVariant,
} from '@xyflow/react'
import '@xyflow/react/dist/style.css'
import { useApp } from '@/lib/store'
import type { NetworkNode, SecurityZone, ZoneRole, NodeRole } from '@/lib/types'

// ─── Visual config ────────────────────────────────────────────────────────────

const ZONE_COLORS: Record<ZoneRole, { bg: string; border: string; text: string }> = {
  lan:     { bg: '#0f2027', border: '#0ea5e9', text: '#7dd3fc' },
  wan:     { bg: '#1a1a2e', border: '#8b5cf6', text: '#c4b5fd' },
  dmz:     { bg: '#1c1107', border: '#f59e0b', text: '#fcd34d' },
  mgmt:    { bg: '#0d1f0d', border: '#22c55e', text: '#86efac' },
  transit: { bg: '#1a1a1a', border: '#6b7280', text: '#d1d5db' },
  oob:     { bg: '#0d1f0d', border: '#10b981', text: '#6ee7b7' },
  guest:   { bg: '#1f0d0d', border: '#f87171', text: '#fca5a5' },
}

const ROLE_ICON: Record<NodeRole, string> = {
  router:   '⬡',
  switch:   '◫',
  firewall: '⬧',
  host:     '□',
  cloud:    '☁',
  ap:       '⊛',
}

const ROLE_COLOR: Record<NodeRole, string> = {
  router:   '#3b82f6',
  switch:   '#22c55e',
  firewall: '#f59e0b',
  host:     '#94a3b8',
  cloud:    '#8b5cf6',
  ap:       '#06b6d4',
}

// ─── Custom node component ────────────────────────────────────────────────────

function DeviceNode({ data }: { data: { node: NetworkNode; zoneColor: string } }) {
  const { node, zoneColor } = data
  const icon = ROLE_ICON[node.role]
  const color = ROLE_COLOR[node.role]
  const ip = node.interfaces.find((i) => i.ipv4_addr)?.ipv4_addr ?? null

  return (
    <div
      className="flex flex-col items-center gap-1 select-none"
      style={{ minWidth: 72 }}
    >
      <div
        className="w-12 h-12 rounded-lg flex items-center justify-center text-xl font-bold shadow-lg border-2"
        style={{ background: `${color}22`, borderColor: color, color }}
      >
        {icon}
      </div>
      <span
        className="text-xs font-semibold text-center leading-tight"
        style={{ color: zoneColor, maxWidth: 80, wordBreak: 'break-all' }}
      >
        {node.label}
      </span>
      {ip && (
        <span className="text-[10px] text-gray-600 font-mono">{ip}</span>
      )}
    </div>
  )
}

const nodeTypes = { device: DeviceNode }

// ─── Layout helpers ───────────────────────────────────────────────────────────

function layoutNodes(
  nodes: NetworkNode[],
  zones: SecurityZone[],
): { flowNodes: Node[]; flowEdges: never[] } {
  // Group nodes by zone
  const byZone = new Map<string, NetworkNode[]>()
  for (const n of nodes) {
    const arr = byZone.get(n.zone_id) ?? []
    arr.push(n)
    byZone.set(n.zone_id, arr)
  }

  const ZONE_PAD = 60
  const NODE_W = 100
  const NODE_H = 100
  const ZONE_GAP = 40

  let zoneX = ZONE_PAD
  const flowNodes: Node[] = []

  for (const zone of zones) {
    const members = byZone.get(zone.id) ?? []
    const cols = Math.max(1, Math.ceil(Math.sqrt(members.length)))
    const rows = Math.max(1, Math.ceil(members.length / cols))
    const zoneW = cols * NODE_W + ZONE_PAD * 2
    const zoneH = rows * NODE_H + ZONE_PAD * 2 + 24 // +24 for label

    const zoneColor = ZONE_COLORS[zone.role] ?? ZONE_COLORS.transit

    // Zone background node
    flowNodes.push({
      id: `zone-${zone.id}`,
      type: 'group',
      position: { x: zoneX, y: 60 },
      style: {
        width: zoneW,
        height: zoneH,
        background: zoneColor.bg,
        border: `1px solid ${zoneColor.border}`,
        borderRadius: 12,
        opacity: 0.9,
      },
      data: { label: zone.label },
      draggable: false,
      selectable: false,
    })

    // Device nodes inside the zone
    members.forEach((n, idx) => {
      const col = idx % cols
      const row = Math.floor(idx / cols)
      flowNodes.push({
        id: n.id,
        type: 'device',
        parentId: `zone-${zone.id}`,
        extent: 'parent',
        position: {
          x: ZONE_PAD + col * NODE_W,
          y: ZONE_PAD + 20 + row * NODE_H,
        },
        data: { node: n, zoneColor: zoneColor.text },
        draggable: true,
      })
    })

    zoneX += zoneW + ZONE_GAP
  }

  return { flowNodes, flowEdges: [] }
}

// ─── Detail panel ─────────────────────────────────────────────────────────────

function NodeDetail({ node, zone }: { node: NetworkNode; zone: SecurityZone | undefined }) {
  return (
    <div className="absolute bottom-4 left-4 w-72 bg-gray-900 border border-gray-700 rounded-xl p-4 text-xs shadow-xl z-10">
      <div className="flex items-center justify-between mb-3">
        <span className="font-bold text-gray-100 text-sm">{node.label}</span>
        <span
          className="px-2 py-0.5 rounded-full text-xs font-medium uppercase"
          style={{
            background: `${ROLE_COLOR[node.role]}22`,
            color: ROLE_COLOR[node.role],
          }}
        >
          {node.role}
        </span>
      </div>
      {zone && (
        <div className="mb-2 text-gray-500">
          Zone: <span className="text-gray-300">{zone.label}</span>
          {' · '}VLAN {zone.vlan ?? '—'}{' · '}Trust {zone.trust_level}
        </div>
      )}
      <div className="space-y-1">
        {node.interfaces.map((iface) => (
          <div key={iface.id} className="flex justify-between font-mono text-gray-400">
            <span className="text-gray-600">{iface.name}</span>
            <span>{iface.ipv4_addr ?? iface.ipv6_addr ?? '—'}</span>
          </div>
        ))}
      </div>
    </div>
  )
}

// ─── Main component ───────────────────────────────────────────────────────────

export default function Topology() {
  const { state } = useApp()
  const graph = state.graph

  const { initialNodes, initialEdges } = useMemo(() => {
    if (!graph) return { initialNodes: [], initialEdges: [] }

    const { flowNodes } = layoutNodes(graph.nodes, graph.zones)

    const flowEdges: Edge[] = graph.edges.map((e) => ({
      id: e.id,
      source: e.from_node,
      target: e.to_node,
      style: { stroke: '#374151', strokeWidth: 2 },
      labelStyle: { fill: '#6b7280', fontSize: 10 },
      label: e.bandwidth ? `${e.bandwidth}M` : undefined,
      type: 'smoothstep',
    }))

    return { initialNodes: flowNodes, initialEdges: flowEdges }
  }, [graph])

  const [nodes, , onNodesChange] = useNodesState(initialNodes)
  const [edges, , onEdgesChange] = useEdgesState(initialEdges)

  // Reset nodes when graph changes
  const selectedNode = useMemo(() => {
    const sel = nodes.find((n) => n.selected && n.type === 'device')
    if (!sel || !graph) return null
    return graph.nodes.find((n) => n.id === sel.id) ?? null
  }, [nodes, graph])

  const selectedZone = useMemo(() => {
    if (!selectedNode || !graph) return undefined
    return graph.zones.find((z) => z.id === selectedNode.zone_id)
  }, [selectedNode, graph])

  const onNodeClick: NodeMouseHandler = useCallback((_e, node) => {
    if (node.type !== 'device') return
  }, [])

  if (!graph || graph.nodes.length === 0) {
    return (
      <div className="flex-1 bg-gray-950 relative flex items-center justify-center">
        <div className="text-center space-y-2">
          <div className="text-gray-700 text-4xl">⬡</div>
          <p className="text-gray-600 text-sm">Topology will appear here as the engine builds your network</p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex-1 bg-gray-950 relative" style={{ height: '100%' }}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={onNodeClick}
        nodeTypes={nodeTypes}
        fitView
        fitViewOptions={{ padding: 0.15 }}
        minZoom={0.2}
        maxZoom={3}
        proOptions={{ hideAttribution: true }}
      >
        <Background variant={BackgroundVariant.Dots} gap={24} size={1} color="#1f2937" />
        <Controls
          className="!bg-gray-900 !border-gray-700 !rounded-lg"
          showInteractive={false}
        />
        <MiniMap
          nodeColor={(n) => {
            if (n.type === 'device') {
              const gn = graph.nodes.find((x) => x.id === n.id)
              return gn ? ROLE_COLOR[gn.role] : '#374151'
            }
            return '#1f2937'
          }}
          maskColor="rgba(0,0,0,0.6)"
          className="!bg-gray-900 !border-gray-700 !rounded-lg"
        />
      </ReactFlow>

      {/* Zone legend */}
      <div className="absolute top-3 right-3 bg-gray-900/90 border border-gray-800 rounded-lg p-3 text-xs space-y-1.5 z-10 backdrop-blur-sm">
        {graph.zones.map((z) => {
          const c = ZONE_COLORS[z.role] ?? ZONE_COLORS.transit
          return (
            <div key={z.id} className="flex items-center gap-2">
              <span className="w-2.5 h-2.5 rounded-sm flex-shrink-0" style={{ background: c.border }} />
              <span style={{ color: c.text }}>{z.label}</span>
            </div>
          )
        })}
      </div>

      {selectedNode && (
        <NodeDetail node={selectedNode} zone={selectedZone} />
      )}
    </div>
  )
}
