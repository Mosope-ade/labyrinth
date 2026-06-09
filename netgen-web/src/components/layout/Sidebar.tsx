'use client'

import { useState, useCallback } from 'react'
import { useApp } from '@/lib/store'
import type { ActiveTab } from '@/lib/store'
import type { Project } from '@/lib/types'

// ─── Icons ────────────────────────────────────────────────────────────────────

const icons = {
  topology: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="3" r="2" stroke="currentColor" strokeWidth="1.3"/>
      <circle cx="3" cy="12" r="2" stroke="currentColor" strokeWidth="1.3"/>
      <circle cx="13" cy="12" r="2" stroke="currentColor" strokeWidth="1.3"/>
      <line x1="8" y1="5" x2="3" y2="10" stroke="currentColor" strokeWidth="1.2"/>
      <line x1="8" y1="5" x2="13" y2="10" stroke="currentColor" strokeWidth="1.2"/>
    </svg>
  ),
  configs: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <rect x="2" y="2" width="12" height="12" rx="2" stroke="currentColor" strokeWidth="1.3"/>
      <line x1="5" y1="6" x2="11" y2="6" stroke="currentColor" strokeWidth="1.2"/>
      <line x1="5" y1="9" x2="9" y2="9" stroke="currentColor" strokeWidth="1.2"/>
    </svg>
  ),
  diagram: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <rect x="1.5" y="1.5" width="13" height="13" rx="2.5" stroke="currentColor" strokeWidth="1.3"/>
      <rect x="4" y="4" width="3" height="3" rx="0.8" fill="currentColor" opacity="0.5"/>
      <rect x="9" y="4" width="3" height="3" rx="0.8" fill="currentColor" opacity="0.5"/>
      <rect x="4" y="9" width="3" height="3" rx="0.8" fill="currentColor" opacity="0.5"/>
      <rect x="9" y="9" width="3" height="3" rx="0.8" fill="currentColor" opacity="0.5"/>
    </svg>
  ),
  ipam: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <rect x="2" y="3" width="5" height="4" rx="1" stroke="currentColor" strokeWidth="1.2"/>
      <rect x="9" y="3" width="5" height="4" rx="1" stroke="currentColor" strokeWidth="1.2"/>
      <rect x="2" y="9" width="5" height="4" rx="1" stroke="currentColor" strokeWidth="1.2"/>
      <rect x="9" y="9" width="5" height="4" rx="1" stroke="currentColor" strokeWidth="1.2"/>
    </svg>
  ),
  security: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <path d="M8 2L13 4.5V9C13 11.8 10.8 14.2 8 15C5.2 14.2 3 11.8 3 9V4.5L8 2Z"
        stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/>
    </svg>
  ),
  simulation: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <polyline points="2,11 5,7 8,9 11,4 14,6" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  history: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="5.5" stroke="currentColor" strokeWidth="1.3"/>
      <polyline points="8,5 8,8 10.5,10" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
    </svg>
  ),
  settings: (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="5.5" stroke="currentColor" strokeWidth="1.3"/>
      <circle cx="8" cy="8" r="2" stroke="currentColor" strokeWidth="1.2"/>
    </svg>
  ),
  plus: (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
      <line x1="7" y1="2" x2="7" y2="12" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
      <line x1="2" y1="7" x2="12" y2="7" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/>
    </svg>
  ),
}

// ─── Nav item ─────────────────────────────────────────────────────────────────

interface NavItemProps {
  icon: React.ReactNode
  label: string
  active?: boolean
  onClick: () => void
}

function NavItem({ icon, label, active, onClick }: NavItemProps) {
  const [hovered, setHovered] = useState(false)

  return (
    <div style={{ position: 'relative' }}>
      <button
        onClick={onClick}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        title={label}
        style={{
          width: 36, height: 36, borderRadius: 8,
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          border: 'none', cursor: 'pointer',
          background: active ? '#edf0ff' : hovered ? '#f0ede6' : 'transparent',
          color: active ? '#4340b0' : hovered ? '#1a1a18' : '#888780',
          transition: 'all 0.1s ease',
        }}
      >
        {icon}
      </button>
      {hovered && (
        <div style={{
          position: 'absolute', left: 46, top: '50%', transform: 'translateY(-50%)',
          background: '#1a1a18', color: '#ffffff',
          fontSize: 11, fontWeight: 500, whiteSpace: 'nowrap',
          padding: '4px 8px', borderRadius: 5,
          pointerEvents: 'none', zIndex: 50,
        }}>
          {label}
        </div>
      )}
    </div>
  )
}

// ─── Status dot ───────────────────────────────────────────────────────────────

function StatusDot({ status }: { status: string }) {
  const color = {
    ready: '#4ade80',
    generating: '#fbbf24',
    draft: '#9ca3af',
    error: '#f87171',
  }[status] ?? '#9ca3af'

  return (
    <div style={{
      width: 6, height: 6, borderRadius: '50%',
      background: color, flexShrink: 0,
    }} />
  )
}

// ─── Relative time ────────────────────────────────────────────────────────────

function timeAgo(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime()
  const m = Math.floor(ms / 60000)
  if (m < 1) return 'just now'
  if (m < 60) return `${m}m ago`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h}h ago`
  return `${Math.floor(h / 24)}d ago`
}

// ─── History panel ────────────────────────────────────────────────────────────

interface HistoryPanelProps {
  onLoad: (project: Project) => void
  onNew: () => void
  activeProjectId: string | null
}

function HistoryPanel({ onLoad, onNew, activeProjectId }: HistoryPanelProps) {
  const { state } = useApp()

  return (
    <div style={{
      position: 'absolute',
      left: 48, top: 0, bottom: 0,
      width: 232,
      background: '#ffffff',
      borderRight: '1px solid #e8e6e0',
      display: 'flex', flexDirection: 'column',
      zIndex: 40,
      boxShadow: '4px 0 12px rgba(26,26,24,0.06)',
    }}>
      {/* Header */}
      <div style={{
        padding: '14px 14px 10px',
        borderBottom: '1px solid #f0ede6',
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      }}>
        <span style={{ fontSize: 11, fontWeight: 600, color: '#5f5e5a', textTransform: 'uppercase', letterSpacing: '0.06em' }}>
          Projects
        </span>
        <button
          onClick={onNew}
          title="New design"
          style={{
            display: 'flex', alignItems: 'center', gap: 4,
            padding: '4px 8px', borderRadius: 6,
            border: '1px solid #e0ddd6', background: '#f5f4f1',
            cursor: 'pointer', fontSize: 11, color: '#5f5e5a',
            fontWeight: 500,
          }}
        >
          {icons.plus} New
        </button>
      </div>

      {/* Project list */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '6px 0' }}>
        {state.projects.length === 0 ? (
          <div style={{
            padding: '24px 16px', textAlign: 'center',
            fontSize: 12, color: '#b4b2a9', lineHeight: 1.5,
          }}>
            No saved designs yet.{'\n'}
            Chat with the AI to create one.
          </div>
        ) : (
          state.projects.map(p => (
            <ProjectRow
              key={p.id}
              project={p}
              active={p.id === activeProjectId}
              onLoad={onLoad}
            />
          ))
        )}
      </div>
    </div>
  )
}

function ProjectRow({ project, active, onLoad }: { project: Project; active: boolean; onLoad: (p: Project) => void }) {
  const [hovered, setHovered] = useState(false)

  return (
    <button
      onClick={() => onLoad(project)}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        display: 'flex', alignItems: 'center', gap: 9,
        width: '100%', padding: '8px 14px',
        border: 'none', cursor: 'pointer', textAlign: 'left',
        background: active ? '#f0efff' : hovered ? '#f8f7f4' : 'transparent',
        borderLeft: active ? '2px solid #4340b0' : '2px solid transparent',
        transition: 'background 0.1s ease',
      }}
    >
      <StatusDot status={project.status} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{
          fontSize: 12, fontWeight: active ? 600 : 400,
          color: active ? '#4340b0' : '#1a1a18',
          overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
        }}>
          {project.name}
        </div>
        <div style={{ fontSize: 10, color: '#b4b2a9', marginTop: 1 }}>
          {timeAgo(project.updated_at)}
        </div>
      </div>
    </button>
  )
}

// ─── Sidebar ─────────────────────────────────────────────────────────────────

const TAB_NAV: { icon: React.ReactNode; label: string; tab: ActiveTab }[] = [
  { icon: icons.topology,   label: 'Topology',   tab: 'topology'   },
  { icon: icons.diagram,    label: 'Diagram',    tab: 'diagram'    },
  { icon: icons.configs,    label: 'Configs',    tab: 'configs'    },
  { icon: icons.ipam,       label: 'IP Plan',    tab: 'ip-plan'    },
  { icon: icons.security,   label: 'Security',   tab: 'audit'      },
  { icon: icons.simulation, label: 'Simulation', tab: 'simulation' },
]

interface SidebarProps {
  onLoadProject?: (project: Project) => void
  onNewDesign?: () => void
}

export default function Sidebar({ onLoadProject, onNewDesign }: SidebarProps) {
  const { state, dispatch } = useApp()
  const [historyOpen, setHistoryOpen] = useState(false)

  const isAuthenticated = state.token && state.token !== 'guest'

  const handleLoad = useCallback((project: Project) => {
    setHistoryOpen(false)
    onLoadProject?.(project)
  }, [onLoadProject])

  const handleNew = useCallback(() => {
    setHistoryOpen(false)
    onNewDesign?.()
  }, [onNewDesign])

  return (
    <aside style={{
      width: 48, flexShrink: 0, position: 'relative',
      background: '#ffffff',
      borderRight: '1px solid #e8e6e0',
      display: 'flex', flexDirection: 'column',
      alignItems: 'center',
      padding: '8px 0',
      gap: 2,
      zIndex: 41,
    }}>
      {TAB_NAV.map(({ icon, label, tab }) => (
        <NavItem
          key={tab}
          icon={icon}
          label={label}
          active={state.activeTab === tab}
          onClick={() => {
            dispatch({ type: 'TAB_SET', tab })
            setHistoryOpen(false)
          }}
        />
      ))}

      <div style={{ flex: 1 }} />
      <div style={{ width: 20, height: 1, background: '#e8e6e0', margin: '4px 0' }} />

      {/* History button — only for authenticated users */}
      {isAuthenticated && (
        <NavItem
          icon={icons.history}
          label="Projects"
          active={historyOpen}
          onClick={() => setHistoryOpen(v => !v)}
        />
      )}

      <NavItem
        icon={icons.settings}
        label="Settings"
        onClick={() => setHistoryOpen(false)}
      />

      {/* History fly-out */}
      {historyOpen && isAuthenticated && (
        <HistoryPanel
          onLoad={handleLoad}
          onNew={handleNew}
          activeProjectId={state.projectId}
        />
      )}
    </aside>
  )
}
