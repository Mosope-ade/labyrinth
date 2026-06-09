'use client'

import { useMemo, useCallback } from 'react'
import {
  ReactFlow,
  Background,
  BackgroundVariant,
  type Node,
  type Edge,
  type NodeMouseHandler,
} from '@xyflow/react'
import '@xyflow/react/dist/style.css'

import { useApp } from '@/lib/store'
import type { TopologyGraph, NetworkNode } from '@/lib/types'
import type { NodeRole, ZoneRole } from '@/src/components/topology/icons'
import DeviceNodeComponent from './nodes/DeviceNode'
import ZoneNodeComponent from './nodes/ZoneNode'
import CanvasToolbar from './CanvasToolbar'
import BuildTimeline from './BuildTimeline'
import { ReactFlowProvider } from '@xyflow/react'

// ─── Role mappings ────────────────────────────────────────────────────────────

function mapNodeRole(role: string): NodeRole {
  const m: Record<string, NodeRole> = {
    router:   'Router',
    switch:   'L3Switch',
    firewall: 'Firewall',
    host:     'Server',
    cloud:    'Cloud',
    ap:       'Ap',
  }
  return m[role] ?? 'Router'
}

function mapZoneRole(role: string): ZoneRole {
  const first = role.charAt(0).toUpperCase() + role.slice(1)
  const valid: ZoneRole[] = ['Lan', 'Wan', 'Dmz', 'Mgmt', 'Transit', 'Oob', 'Guest']
  return valid.includes(first as ZoneRole) ? (first as ZoneRole) : 'Lan'
}

// ─── Auto-layout ─────────────────────────────────────────────────────────────

const ZONE_PADDING_X = 28
const ZONE_PADDING_Y = 48
const NODE_W = 100
const NODE_H = 90
const ZONE_GAP = 32
const ZONE_MIN_W = 180

function buildLayout(graph: TopologyGraph) {
  const nodes: Node[] = []
  const edges: Edge[] = []

  let xOffset = 16

  const subnetByZone: Record<string, string> = {}
  for (const a of graph.addressing.ipv4_allocations) {
    if (!subnetByZone[a.zone_id]) subnetByZone[a.zone_id] = a.address + '/' + a.prefix_len
  }

  const ipByNode: Record<string, string> = {}
  for (const node of graph.nodes) {
    const iface = node.interfaces.find(i => i.ipv4_addr)
    if (iface?.ipv4_addr) ipByNode[node.id] = iface.ipv4_addr.split('/')[0]
  }

  for (const zone of graph.zones) {
    const zoneNodes = graph.nodes.filter((n: NetworkNode) => n.zone_id === zone.id)
    const cols = Math.max(1, Math.min(zoneNodes.length, 4))
    const rows = Math.ceil(zoneNodes.length / cols)
    const zoneW = Math.max(ZONE_MIN_W, ZONE_PADDING_X * 2 + cols * NODE_W)
    const zoneH = ZONE_PADDING_Y + rows * NODE_H + 16

    const zoneId = `zone-${zone.id}`

    nodes.push({
      id: zoneId,
      type: 'zone',
      position: { x: xOffset, y: 16 },
      data: {
        name: zone.name,
        role: mapZoneRole(zone.role),
        subnet: subnetByZone[zone.id],
      },
      style: { width: zoneW, height: zoneH },
      selectable: false,
      draggable: false,
      zIndex: 0,
    })

    zoneNodes.forEach((node: NetworkNode, i: number) => {
      const col = i % cols
      const row = Math.floor(i / cols)
      nodes.push({
        id: node.id,
        type: 'device',
        position: {
          x: ZONE_PADDING_X + col * NODE_W,
          y: ZONE_PADDING_Y + row * NODE_H,
        },
        data: {
          label: node.label,
          role: mapNodeRole(node.role),
          ip: ipByNode[node.id],
        },
        parentId: zoneId,
        extent: 'parent' as const,
        zIndex: 2,
      })
    })

    xOffset += zoneW + ZONE_GAP
  }

  for (const link of graph.edges) {
    edges.push({
      id: link.id,
      source: link.from_node,
      target: link.to_node,
      style: { stroke: '#d8d6cc', strokeWidth: 1.5 },
      type: 'default',
    })
  }

  return { nodes, edges }
}

// ─── Node types ───────────────────────────────────────────────────────────────

const NODE_TYPES = {
  device: DeviceNodeComponent,
  zone:   ZoneNodeComponent,
}

// ─── Empty state ──────────────────────────────────────────────────────────────

function EmptyCanvas() {
  return (
    <div style={{
      position: 'absolute', inset: 0,
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      flexDirection: 'column', gap: 12,
      pointerEvents: 'none',
    }}>
      <div style={{
        width: 48, height: 48, borderRadius: 12,
        background: '#f0ede6',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
      }}>
        <svg width="22" height="22" viewBox="0 0 22 22" fill="none">
          <circle cx="11" cy="6" r="3" stroke="#888780" strokeWidth="1.4"/>
          <circle cx="5"  cy="17" r="3" stroke="#888780" strokeWidth="1.4"/>
          <circle cx="17" cy="17" r="3" stroke="#888780" strokeWidth="1.4"/>
          <line x1="11" y1="9" x2="5"  y2="14" stroke="#888780" strokeWidth="1.3"/>
          <line x1="11" y1="9" x2="17" y2="14" stroke="#888780" strokeWidth="1.3"/>
        </svg>
      </div>
      <div style={{ textAlign: 'center' }}>
        <p style={{ fontSize: 13, fontWeight: 500, color: '#5f5e5a', margin: 0 }}>
          No topology yet
        </p>
        <p style={{ fontSize: 11, color: '#888780', marginTop: 4 }}>
          Describe your network in the chat to get started
        </p>
      </div>
    </div>
  )
}

// ─── Inner canvas (needs ReactFlowProvider context) ───────────────────────────

function CanvasInner({ graph }: { graph: TopologyGraph | null }) {
  const { dispatch } = useApp()
  const { nodes, edges } = useMemo(
    () => graph ? buildLayout(graph) : { nodes: [], edges: [] },
    [graph]
  )

  const onNodeClick: NodeMouseHandler = useCallback((_evt, node) => {
    if (node.type === 'device') {
      dispatch({ type: 'NODE_SELECT', id: node.id })
    }
  }, [dispatch])

  return (
    <div style={{ flex: 1, position: 'relative', background: '#fafaf8' }}>
      {!graph && <EmptyCanvas />}

      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={NODE_TYPES}
        fitView={!!graph}
        fitViewOptions={{ padding: 0.15 }}
        proOptions={{ hideAttribution: true }}
        panOnScroll
        selectionOnDrag={false}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={!!graph}
        onNodeClick={onNodeClick}
        style={{ background: 'transparent' }}
      >
        <Background
          variant={BackgroundVariant.Dots}
          gap={20}
          size={1}
          color="#d8d6cc"
          style={{ opacity: 0.5 }}
        />
        <BuildTimeline />
      </ReactFlow>

      {graph && (
        <div style={{ position: 'absolute', bottom: 0, left: 0, right: 0, zIndex: 5 }}>
          <CanvasToolbar />
        </div>
      )}
    </div>
  )
}

// ─── Public export ────────────────────────────────────────────────────────────

export default function TopologyCanvas() {
  const { state } = useApp()

  return (
    <ReactFlowProvider>
      <CanvasInner graph={state.graph} />
    </ReactFlowProvider>
  )
}
