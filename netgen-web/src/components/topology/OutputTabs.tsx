'use client'

import { useApp } from '@/lib/store'
import type { ActiveTab } from '@/lib/store'

const TABS: { id: ActiveTab; label: string }[] = [
  { id: 'topology',   label: 'Topology'   },
  { id: 'diagram',    label: 'Diagram'    },
  { id: 'configs',    label: 'Configs'    },
  { id: 'ip-plan',    label: 'IP Plan'    },
  { id: 'audit',      label: 'Security'   },
  { id: 'simulation', label: 'Simulation' },
]

export default function OutputTabs() {
  const { state, dispatch } = useApp()

  const configCount = state.graph?.nodes.filter(n => n.config).length ?? 0

  return (
    <div style={{
      height: 36, flexShrink: 0,
      background: '#ffffff',
      borderBottom: '1px solid #e8e6e0',
      display: 'flex', alignItems: 'stretch',
      padding: '0 4px',
    }}>
      {TABS.map((tab) => {
        const active = state.activeTab === tab.id
        const badge = tab.id === 'configs' && configCount > 0 ? configCount : null

        return (
          <button
            key={tab.id}
            onClick={() => dispatch({ type: 'TAB_SET', tab: tab.id })}
            style={{
              display: 'flex', alignItems: 'center', gap: 5,
              padding: '0 10px',
              border: 'none', borderBottom: active ? '2px solid #1a1a18' : '2px solid transparent',
              background: 'transparent',
              fontSize: 11, fontWeight: 500,
              color: active ? '#1a1a18' : '#888780',
              cursor: 'pointer',
              transition: 'color 0.1s ease',
              marginBottom: -1,
              whiteSpace: 'nowrap',
            }}
          >
            {tab.label}
            {badge !== null && (
              <span style={{
                background: '#edf0ff', color: '#4340b0',
                fontSize: 9, fontWeight: 600,
                padding: '1px 5px', borderRadius: 9999,
              }}>
                {badge}
              </span>
            )}
          </button>
        )
      })}
    </div>
  )
}
