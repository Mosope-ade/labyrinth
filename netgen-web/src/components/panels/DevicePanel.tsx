'use client'

import { useState } from 'react'
import { useApp } from '@/lib/store'
import type { NetworkNode } from '@/lib/types'
import { NODE_ICON_REGISTRY } from '@/src/components/topology/icons'
import type { NodeRole } from '@/src/components/topology/icons'

type DeviceTab = 'interfaces' | 'routing' | 'config'

const ROLE_MAP: Record<string, NodeRole> = {
  router:   'Router',
  switch:   'L3Switch',
  firewall: 'Firewall',
  host:     'Server',
  cloud:    'Cloud',
  ap:       'Ap',
}

interface DevicePanelContentProps {
  node: NetworkNode
}

function DevicePanelContent({ node }: DevicePanelContentProps) {
  const [tab, setTab] = useState<DeviceTab>('interfaces')
  const { state, dispatch } = useApp()

  const roleKey: NodeRole = ROLE_MAP[node.role] ?? 'Router'
  const Icon = NODE_ICON_REGISTRY[roleKey] ?? NODE_ICON_REGISTRY['Router']

  const zone = state.graph?.zones.find(z => z.id === node.zone_id)
  const firstIp = node.interfaces.find(i => i.ipv4_addr)?.ipv4_addr?.split('/')[0]

  const tabs: { id: DeviceTab; label: string }[] = [
    { id: 'interfaces', label: 'Interfaces' },
    { id: 'routing',    label: 'Routing' },
    { id: 'config',     label: 'Config' },
  ]

  return (
    <div style={{
      height: 200, flexShrink: 0,
      background: '#ffffff',
      borderTop: '1px solid #e8e6e0',
      display: 'flex', flexDirection: 'column',
    }}>
      {/* Header */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 10,
        padding: '8px 14px', borderBottom: '1px solid #e8e6e0',
        flexShrink: 0,
      }}>
        <Icon size={28} />
        <div style={{ flex: 1 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ fontSize: 13, fontWeight: 600, color: '#1a1a18' }}>{node.label}</span>
            <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
              <div style={{ width: 5, height: 5, borderRadius: '50%', background: '#1d9e75' }} />
              <span style={{ fontSize: 10, color: '#0f6e56' }}>Online</span>
            </div>
          </div>
          <div style={{ display: 'flex', gap: 16, marginTop: 2 }}>
            {[
              ['Role',   node.role],
              ['Vendor', node.vendor],
              ['Zone',   zone?.name ?? '—'],
              ['IP',     firstIp ?? '—'],
              ['VLAN',   zone?.vlan?.toString() ?? '—'],
            ].map(([label, value]) => (
              <div key={label}>
                <div style={{ fontSize: 10, textTransform: 'uppercase', letterSpacing: '0.05em', color: '#888780' }}>
                  {label}
                </div>
                <div style={{
                  fontSize: 11, fontWeight: 500, color: '#1a1a18',
                  fontFamily: label === 'IP' ? 'var(--font-mono), monospace' : undefined,
                }}>
                  {value}
                </div>
              </div>
            ))}
          </div>
        </div>
        <button
          onClick={() => dispatch({ type: 'NODE_SELECT', id: null })}
          style={{ background: 'none', border: 'none', cursor: 'pointer', color: '#888780', padding: 4 }}
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M3 3L11 11M11 3L3 11" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
          </svg>
        </button>
      </div>

      {/* Tabs */}
      <div style={{
        display: 'flex', borderBottom: '1px solid #e8e6e0',
        padding: '0 14px', flexShrink: 0,
      }}>
        {tabs.map(t => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            style={{
              padding: '6px 10px',
              fontSize: 11, fontWeight: 500,
              border: 'none', background: 'none', cursor: 'pointer',
              borderBottom: tab === t.id ? '2px solid #1a1a18' : '2px solid transparent',
              color: tab === t.id ? '#1a1a18' : '#888780',
              marginBottom: -1,
            }}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Content */}
      <div style={{ flex: 1, overflow: 'auto', padding: '8px 14px' }}>
        {tab === 'interfaces' && (
          <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 11 }}>
            <thead>
              <tr>
                {['Interface', 'IPv4', 'Role', 'Status'].map(h => (
                  <th key={h} style={{
                    textAlign: 'left', fontSize: 10,
                    textTransform: 'uppercase', letterSpacing: '0.05em',
                    color: '#888780', paddingBottom: 4, fontWeight: 500,
                  }}>
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {node.interfaces.map(iface => (
                <tr key={iface.id} style={{ borderTop: '1px solid #f0ede6' }}>
                  <td style={{ padding: '4px 0', fontFamily: 'var(--font-mono), monospace', color: '#1a1a18' }}>
                    {iface.name}
                  </td>
                  <td style={{ padding: '4px 0', fontFamily: 'var(--font-mono), monospace', color: '#5f5e5a' }}>
                    {iface.ipv4_addr ?? '—'}
                  </td>
                  <td style={{ padding: '4px 0', color: '#888780' }}>{iface.role}</td>
                  <td style={{ padding: '4px 0' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                      <div style={{ width: 4, height: 4, borderRadius: '50%', background: '#1d9e75' }} />
                      <span style={{ color: '#0f6e56', fontSize: 10 }}>Up</span>
                    </div>
                  </td>
                </tr>
              ))}
              {node.interfaces.length === 0 && (
                <tr>
                  <td colSpan={4} style={{ color: '#b4b2a9', padding: '8px 0' }}>No interfaces</td>
                </tr>
              )}
            </tbody>
          </table>
        )}

        {tab === 'config' && node.config && (
          <pre style={{
            margin: 0,
            background: '#1a1a18', color: '#9fe1cb',
            fontSize: 10, fontFamily: 'var(--font-mono), monospace',
            padding: '8px 10px', borderRadius: 6,
            overflow: 'auto', whiteSpace: 'pre',
          }}>
            {node.config.raw}
          </pre>
        )}

        {tab === 'config' && !node.config && (
          <p style={{ color: '#b4b2a9', fontSize: 11 }}>Config not yet generated.</p>
        )}

        {tab === 'routing' && (
          <p style={{ color: '#b4b2a9', fontSize: 11 }}>Routing info available after build.</p>
        )}
      </div>
    </div>
  )
}

export default function DevicePanel() {
  const { state } = useApp()

  const node = state.selectedNodeId
    ? state.graph?.nodes.find(n => n.id === state.selectedNodeId) ?? null
    : null

  if (!node) return null
  return <DevicePanelContent node={node} />
}
