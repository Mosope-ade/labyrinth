'use client'

import { useState } from 'react'
import { useApp } from '@/lib/store'
import type { DesignDecision, TriggerSource } from '@/lib/types'

// ─── Decision log panel ───────────────────────────────────────────────────────

const TRIGGER_BADGE: Record<TriggerSource, { label: string; color: string }> = {
  user_intent:     { label: 'User',     color: 'text-blue-400 bg-blue-400/10' },
  rule_engine:     { label: 'Rule',     color: 'text-amber-400 bg-amber-400/10' },
  security_policy: { label: 'Security', color: 'text-red-400 bg-red-400/10' },
  ai_inference:    { label: 'AI',       color: 'text-purple-400 bg-purple-400/10' },
}

function DecisionLog({ decisions }: { decisions: DesignDecision[] }) {
  const [filter, setFilter] = useState<TriggerSource | 'all'>('all')
  const filtered = filter === 'all' ? decisions : decisions.filter((d) => d.triggered_by === filter)

  return (
    <div className="flex flex-col h-full">
      <div className="px-4 py-2 border-b border-gray-800 flex items-center gap-2 flex-wrap flex-shrink-0">
        {(['all', 'user_intent', 'rule_engine', 'security_policy', 'ai_inference'] as const).map((f) => (
          <button
            key={f}
            onClick={() => setFilter(f)}
            className={`px-2 py-0.5 rounded text-xs font-medium transition-colors ${
              filter === f ? 'bg-gray-700 text-gray-100' : 'text-gray-500 hover:text-gray-300'
            }`}
          >
            {f === 'all' ? 'All' : TRIGGER_BADGE[f].label}
          </button>
        ))}
        <span className="ml-auto text-xs text-gray-600">{filtered.length} decisions</span>
      </div>
      <div className="flex-1 overflow-y-auto min-h-0">
        {filtered.map((d) => {
          const badge = TRIGGER_BADGE[d.triggered_by]
          return (
            <div key={d.id} className="px-4 py-3 border-b border-gray-800/50 hover:bg-gray-900/50 transition-colors">
              <div className="flex items-start justify-between gap-2 mb-1">
                <span className="text-xs font-mono text-gray-400">{d.component}</span>
                <div className="flex items-center gap-1.5 flex-shrink-0">
                  {d.rule_id && (
                    <span className="text-[10px] font-mono text-gray-600 bg-gray-800 px-1.5 py-0.5 rounded">
                      {d.rule_id}
                    </span>
                  )}
                  <span className={`text-[10px] font-medium px-1.5 py-0.5 rounded ${badge.color}`}>
                    {badge.label}
                  </span>
                  {d.overridable && (
                    <span className="text-[10px] text-gray-600">editable</span>
                  )}
                </div>
              </div>
              <p className="text-xs text-gray-300 leading-relaxed">{d.reason}</p>
            </div>
          )
        })}
        {filtered.length === 0 && (
          <div className="p-8 text-center text-gray-700 text-xs">No decisions</div>
        )}
      </div>
    </div>
  )
}

// ─── IP plan table ────────────────────────────────────────────────────────────

function IpPlan() {
  const { state } = useApp()
  const graph = state.graph
  const allocs = graph?.addressing.ipv4_allocations ?? []

  const byZone = new Map<string, typeof allocs>()
  for (const a of allocs) {
    const arr = byZone.get(a.zone_id) ?? []
    arr.push(a)
    byZone.set(a.zone_id, arr)
  }

  const zones = graph?.zones ?? []

  if (allocs.length === 0) {
    return <div className="p-8 text-center text-gray-700 text-xs">No addressing plan yet</div>
  }

  return (
    <div className="overflow-auto h-full p-4">
      {zones.map((zone) => {
        const zoneAllocs = byZone.get(zone.id)
        if (!zoneAllocs?.length) return null
        return (
          <div key={zone.id} className="mb-6">
            <div className="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-2">
              {zone.label} · VLAN {zone.vlan ?? '—'}
            </div>
            <table className="w-full text-xs font-mono">
              <thead>
                <tr className="text-gray-600 border-b border-gray-800">
                  <th className="text-left pb-1 pr-4 font-medium">Node</th>
                  <th className="text-left pb-1 pr-4 font-medium">Interface</th>
                  <th className="text-left pb-1 font-medium">Address / Prefix</th>
                </tr>
              </thead>
              <tbody>
                {zoneAllocs.map((a, i) => {
                  const node = graph?.nodes.find((n) => n.id === a.node_id)
                  const iface = node?.interfaces.find((f) => f.id === a.interface_id)
                  return (
                    <tr key={i} className="border-b border-gray-800/40 hover:bg-gray-900/30">
                      <td className="py-1.5 pr-4 text-gray-300">{node?.label ?? a.node_id}</td>
                      <td className="py-1.5 pr-4 text-gray-500">{iface?.name ?? a.interface_id}</td>
                      <td className="py-1.5 text-emerald-400">
                        {a.address}/{a.prefix_len}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        )
      })}
    </div>
  )
}

// ─── Zone policy table ────────────────────────────────────────────────────────

function PolicyMatrix() {
  const { state } = useApp()
  const graph = state.graph
  const policies = graph?.policies ?? []
  const zones = graph?.zones ?? []

  const zoneLabel = (id: string) => zones.find((z) => z.id === id)?.label ?? id

  const ACTION_COLOR: Record<string, string> = {
    permit:  'text-emerald-400',
    deny:    'text-red-400',
    inspect: 'text-amber-400',
  }

  if (policies.length === 0) {
    return <div className="p-8 text-center text-gray-700 text-xs">No zone policies yet</div>
  }

  return (
    <div className="overflow-auto h-full p-4">
      <table className="w-full text-xs">
        <thead>
          <tr className="text-gray-600 border-b border-gray-800">
            <th className="text-left pb-2 pr-4 font-medium">From</th>
            <th className="text-left pb-2 pr-4 font-medium">To</th>
            <th className="text-left pb-2 pr-4 font-medium">Action</th>
            <th className="text-left pb-2 font-medium">Services</th>
          </tr>
        </thead>
        <tbody>
          {policies.map((p, i) => (
            <tr key={i} className="border-b border-gray-800/40 hover:bg-gray-900/30">
              <td className="py-2 pr-4 text-gray-300 font-mono">{zoneLabel(p.from_zone)}</td>
              <td className="py-2 pr-4 text-gray-300 font-mono">{zoneLabel(p.to_zone)}</td>
              <td className={`py-2 pr-4 font-semibold uppercase text-[11px] ${ACTION_COLOR[p.action] ?? 'text-gray-400'}`}>
                {p.action}
              </td>
              <td className="py-2 text-gray-500">{p.services.join(', ') || '—'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

// ─── Main ─────────────────────────────────────────────────────────────────────

type SimTab = 'decisions' | 'ip-plan' | 'policies'

export default function Simulation() {
  const { state } = useApp()
  const [tab, setTab] = useState<SimTab>('decisions')
  const graph = state.graph

  if (!graph) {
    return (
      <div className="flex-1 bg-gray-950 flex items-center justify-center">
        <p className="text-gray-600 text-sm">No design loaded</p>
      </div>
    )
  }

  const tabs: { id: SimTab; label: string; count?: number }[] = [
    { id: 'decisions', label: 'Why this?', count: graph.decisions.length },
    { id: 'ip-plan',   label: 'IP Plan',   count: graph.addressing.ipv4_allocations.length },
    { id: 'policies',  label: 'Policies',  count: graph.policies.length },
  ]

  return (
    <div className="flex flex-col h-full bg-gray-950 text-sm">
      {/* Sub-tabs */}
      <div className="flex border-b border-gray-800 px-4 flex-shrink-0">
        {tabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={`px-3 py-2.5 text-xs font-medium border-b-2 transition-colors -mb-px flex items-center gap-1.5 ${
              tab === t.id
                ? 'border-blue-500 text-blue-400'
                : 'border-transparent text-gray-500 hover:text-gray-300'
            }`}
          >
            {t.label}
            {t.count !== undefined && (
              <span className="px-1.5 py-0.5 rounded-full bg-gray-800 text-gray-500 text-[10px]">
                {t.count}
              </span>
            )}
          </button>
        ))}
      </div>

      {/* Panel */}
      <div className="flex-1 min-h-0">
        {tab === 'decisions' && <DecisionLog decisions={graph.decisions} />}
        {tab === 'ip-plan'   && <IpPlan />}
        {tab === 'policies'  && <PolicyMatrix />}
      </div>
    </div>
  )
}
