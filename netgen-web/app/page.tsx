'use client'

import { useCallback, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { useApp } from '@/lib/store'
import { connectJobStream, getProject, getProjectTurns, listProjects, register, login } from '@/lib/api'
import type { Project } from '@/lib/types'

import Topbar from '@/src/components/layout/Topbar'
import Sidebar from '@/src/components/layout/Sidebar'
import OutputTabs from '@/src/components/topology/OutputTabs'
import TopologyCanvas from '@/src/components/topology/TopologyCanvas'
import DevicePanel from '@/src/components/panels/DevicePanel'
import ChatPanel from '@/src/components/chat/ChatPanel'

// ─── Auth modal (rendered via portal so it sits above all stacking contexts) ──

function AuthModal() {
  const { dispatch } = useApp()
  const [mode, setMode] = useState<'login' | 'register'>('login')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  // Defer portal mount to after hydration — avoids server/client mismatch.
  const [mounted, setMounted] = useState(false)
  useEffect(() => { setMounted(true) }, [])

  async function submit(e: React.SyntheticEvent<HTMLFormElement>) {
    e.preventDefault()
    setError(null)
    setLoading(true)
    try {
      const res = mode === 'login'
        ? await login(email, password)
        : await register(email, password)
      localStorage.setItem('laby_token', res.token)
      localStorage.setItem('laby_email', res.email)
      dispatch({ type: 'AUTH_SET', token: res.token, email: res.email })
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Authentication failed')
    } finally {
      setLoading(false)
    }
  }

  const content = (
    <div style={{
      position: 'fixed', inset: 0,
      background: 'rgba(26,26,24,0.7)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      zIndex: 9999, backdropFilter: 'blur(4px)',
    }}>
      <div style={{
        background: '#ffffff',
        border: '1px solid #e8e6e0',
        borderRadius: 16,
        padding: 36,
        width: 360,
        boxShadow: '0 20px 40px rgba(26,26,24,0.12)',
      }}>
        {/* Logo + tagline */}
        <div style={{ marginBottom: 28 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 8 }}>
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
              <rect width="24" height="24" rx="6" fill="#1a1a18"/>
              <circle cx="12" cy="7" r="2.5" fill="#9fe1cb"/>
              <circle cx="7" cy="17" r="2.5" fill="#9fe1cb"/>
              <circle cx="17" cy="17" r="2.5" fill="#9fe1cb"/>
              <line x1="12" y1="9.5" x2="7" y2="14.5" stroke="#9fe1cb" strokeWidth="1.2"/>
              <line x1="12" y1="9.5" x2="17" y2="14.5" stroke="#9fe1cb" strokeWidth="1.2"/>
            </svg>
            <span style={{ fontSize: 17, fontWeight: 700, color: '#1a1a18', letterSpacing: '-0.02em' }}>
              Labyrinth
            </span>
          </div>
          <p style={{ fontSize: 13, color: '#888780', margin: 0 }}>
            From intent to infrastructure
          </p>
        </div>

        <form onSubmit={submit}>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
            <div>
              <label style={{ display: 'block', fontSize: 11, color: '#5f5e5a', marginBottom: 5, fontWeight: 500 }}>
                Email
              </label>
              <input
                type="email"
                value={email}
                onChange={e => setEmail(e.target.value)}
                required
                autoFocus
                placeholder="you@example.com"
                style={{
                  width: '100%', boxSizing: 'border-box',
                  background: '#f5f4f1', border: '1px solid #e0ddd6',
                  borderRadius: 8, padding: '9px 12px',
                  fontSize: 13, color: '#1a1a18',
                  outline: 'none', transition: 'border-color 0.1s',
                }}
              />
            </div>
            <div>
              <label style={{ display: 'block', fontSize: 11, color: '#5f5e5a', marginBottom: 5, fontWeight: 500 }}>
                Password
              </label>
              <input
                type="password"
                value={password}
                onChange={e => setPassword(e.target.value)}
                required
                placeholder="••••••••"
                style={{
                  width: '100%', boxSizing: 'border-box',
                  background: '#f5f4f1', border: '1px solid #e0ddd6',
                  borderRadius: 8, padding: '9px 12px',
                  fontSize: 13, color: '#1a1a18',
                  outline: 'none',
                }}
              />
            </div>

            {error && (
              <div style={{
                fontSize: 12, color: '#b54040',
                background: '#fdf0f0', border: '1px solid #f5cccc',
                borderRadius: 8, padding: '8px 12px',
              }}>
                {error}
              </div>
            )}

            <button
              type="submit"
              disabled={loading}
              style={{
                padding: '10px',
                background: loading ? '#b4b2a9' : '#1a1a18',
                color: '#ffffff',
                border: 'none', borderRadius: 8,
                fontSize: 13, fontWeight: 600, cursor: loading ? 'default' : 'pointer',
              }}
            >
              {loading ? '…' : mode === 'login' ? 'Sign in' : 'Create account'}
            </button>
          </div>
        </form>

        <div style={{ marginTop: 16, display: 'flex', flexDirection: 'column', gap: 8, alignItems: 'center' }}>
          <button
            onClick={() => { setMode(mode === 'login' ? 'register' : 'login'); setError(null) }}
            style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 12, color: '#888780' }}
          >
            {mode === 'login' ? 'No account? Create one' : 'Have an account? Sign in'}
          </button>
          <button
            onClick={() => dispatch({ type: 'AUTH_SET', token: 'guest', email: 'guest' })}
            style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 11, color: '#b4b2a9' }}
          >
            Continue as guest
          </button>
        </div>
      </div>
    </div>
  )

  if (!mounted) return null
  return createPortal(content, document.body)
}

// ─── Root workspace layout ────────────────────────────────────────────────────

export default function Home() {
  const { state, dispatch } = useApp()
  const wsRef = useRef<WebSocket | null>(null)
  const [chatCollapsed, setChatCollapsed] = useState(false)

  // Fetch project history whenever a real user signs in
  useEffect(() => {
    const isRealUser = state.token && state.token !== 'guest'
    if (!isRealUser) return
    listProjects()
      .then(projects => dispatch({ type: 'PROJECTS_SET', projects }))
      .catch(() => {}) // non-critical
  }, [state.token]) // eslint-disable-line react-hooks/exhaustive-deps

  // Job stream for the active project
  useEffect(() => {
    if (!state.project) return
    if (wsRef.current) wsRef.current.close()

    wsRef.current = connectJobStream(state.project.id, (event) => {
      dispatch({ type: 'JOB_EVENT', event })
      if (event.type === 'complete' && state.project) {
        getProject(state.project.id).then((p) => {
          dispatch({ type: 'PROJECT_SET', project: p })
          if (p.graph_json) dispatch({ type: 'GRAPH_SET', graph: p.graph_json })
          // Refresh project list so history panel shows updated status
          listProjects()
            .then(projects => dispatch({ type: 'PROJECTS_SET', projects }))
            .catch(() => {})
        })
      }
    })

    return () => { wsRef.current?.close(); wsRef.current = null }
  }, [state.project?.id]) // eslint-disable-line react-hooks/exhaustive-deps

  // Load a project from history: fetch detail + turns and restore UI state
  const handleLoadProject = useCallback(async (project: Project) => {
    try {
      const [detail, turns] = await Promise.all([
        getProject(project.id),
        getProjectTurns(project.id),
      ])
      dispatch({
        type: 'PROJECT_LOAD',
        project,
        graph: detail.graph_json,
        turns: turns.filter(t => t.role === 'user' || t.role === 'assistant'),
      })
    } catch {
      // Silently ignore — UI stays unchanged if load fails
    }
  }, [dispatch])

  // Start a fresh design session
  const handleNewDesign = useCallback(() => {
    dispatch({ type: 'AUTH_CLEAR' })
    // Re-set the token after clearing so user stays logged in
    const token = localStorage.getItem('laby_token')
    const email = localStorage.getItem('laby_email')
    if (token && email) {
      dispatch({ type: 'AUTH_SET', token, email })
    }
    listProjects()
      .then(projects => dispatch({ type: 'PROJECTS_SET', projects }))
      .catch(() => {})
  }, [dispatch])

  const showAuth = !state.token && !state.userEmail

  return (
    <>
      {showAuth && <AuthModal />}
      <div style={{
        display: 'flex', flexDirection: 'column',
        height: '100vh', overflow: 'hidden',
        background: '#f5f4f1',
      }}>
        {/* Topbar */}
        <Topbar />

        {/* Main area below topbar */}
        <div style={{ display: 'flex', flex: 1, minHeight: 0 }}>

          {/* Sidebar */}
          <Sidebar onLoadProject={handleLoadProject} onNewDesign={handleNewDesign} />

          {/* Center: tabs + canvas + device panel */}
          <div style={{ display: 'flex', flexDirection: 'column', flex: 1, minWidth: 0, minHeight: 0 }}>
            <OutputTabs />

            {/* Canvas area — shown when topology tab is active */}
            {state.activeTab === 'topology' && (
              <div style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}>
                <div style={{ flex: 1, minHeight: 0 }}>
                  <TopologyCanvas />
                </div>
                <DevicePanel />
              </div>
            )}

            {/* Other output panels */}
            {state.activeTab !== 'topology' && (
              <div style={{ flex: 1, minHeight: 0, overflow: 'auto', padding: 24 }}>
                <OtherPanel tab={state.activeTab} />
              </div>
            )}
          </div>

          {/* Chat panel */}
          <ChatPanel
            collapsed={chatCollapsed}
            onToggle={() => setChatCollapsed(v => !v)}
          />
        </div>
      </div>
    </>
  )
}

// ─── Placeholder panels for non-topology tabs ─────────────────────────────────

import type { ActiveTab } from '@/lib/store'

function OtherPanel({ tab }: { tab: ActiveTab }) {
  const { state } = useApp()

  if (tab === 'configs') {
    if (!state.graph) {
      return <EmptyState label="Configs" hint="Generate a topology to produce configs" />
    }
    const nodes = state.graph.nodes.filter(n => n.config)
    if (nodes.length === 0) {
      return <EmptyState label="Configs" hint="Configs will appear here after the engine runs" />
    }
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
        {nodes.map(n => (
          <div key={n.id}>
            <div style={{ fontSize: 11, fontWeight: 600, color: '#5f5e5a', marginBottom: 6 }}>
              {n.label} ({n.vendor})
            </div>
            <pre style={{
              background: '#1a1a18', color: '#9fe1cb',
              fontSize: 11, fontFamily: 'var(--font-mono), monospace',
              padding: '12px 14px', borderRadius: 8,
              overflow: 'auto', whiteSpace: 'pre',
              margin: 0,
            }}>
              {n.config!.raw}
            </pre>
          </div>
        ))}
      </div>
    )
  }

  if (tab === 'ip-plan') {
    if (!state.graph) return <EmptyState label="IP Plan" hint="Generate a topology to see the addressing plan" />
    const { ipv4_allocations } = state.graph.addressing
    return (
      <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 12 }}>
        <thead>
          <tr>
            {['Zone', 'Address', 'Prefix', 'Node', 'Interface'].map(h => (
              <th key={h} style={{
                textAlign: 'left', fontSize: 10, textTransform: 'uppercase',
                letterSpacing: '0.05em', color: '#888780',
                padding: '0 0 10px 0', fontWeight: 500,
                borderBottom: '1px solid #e8e6e0',
              }}>
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {ipv4_allocations.map((a, i) => {
            const zone = state.graph!.zones.find(z => z.id === a.zone_id)
            const node = state.graph!.nodes.find(n => n.id === a.node_id)
            const iface = node?.interfaces.find(f => f.id === a.interface_id)
            return (
              <tr key={i} style={{ borderBottom: '1px solid #f0ede6' }}>
                <td style={{ padding: '7px 0', color: '#5f5e5a' }}>{zone?.name ?? a.zone_id}</td>
                <td style={{ padding: '7px 0', fontFamily: 'var(--font-mono), monospace', color: '#1a1a18' }}>{a.address}</td>
                <td style={{ padding: '7px 0', fontFamily: 'var(--font-mono), monospace', color: '#888780' }}>/{a.prefix_len}</td>
                <td style={{ padding: '7px 0', color: '#5f5e5a' }}>{node?.label ?? a.node_id}</td>
                <td style={{ padding: '7px 0', fontFamily: 'var(--font-mono), monospace', color: '#888780' }}>{iface?.name ?? a.interface_id}</td>
              </tr>
            )
          })}
        </tbody>
      </table>
    )
  }

  if (tab === 'audit') {
    if (!state.graph) return <EmptyState label="Decision Log" hint="Decisions will be logged as the engine builds" />
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        {state.graph.decisions.map(d => (
          <div key={d.id} style={{
            background: '#ffffff', border: '1px solid #e8e6e0',
            borderRadius: 8, padding: '12px 16px',
          }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
              <div>
                <div style={{ fontSize: 12, fontWeight: 600, color: '#1a1a18' }}>{d.component}</div>
                <div style={{ fontSize: 12, color: '#5f5e5a', marginTop: 3, lineHeight: 1.5 }}>{d.reason}</div>
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-end', gap: 4 }}>
                <div style={{
                  fontSize: 10, padding: '2px 7px', borderRadius: 4,
                  background: '#f0ede6', color: '#888780',
                  fontWeight: 500, textTransform: 'uppercase', letterSpacing: '0.04em',
                }}>
                  {d.triggered_by.replace('_', ' ')}
                </div>
                {d.rule_id && (
                  <div style={{ fontSize: 10, color: '#b4b2a9', fontFamily: 'var(--font-mono), monospace' }}>
                    {d.rule_id}
                  </div>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    )
  }

  if (tab === 'simulation') {
    return <EmptyState label="Simulation" hint="Route simulation available after topology builds" />
  }

  return null
}

function EmptyState({ label, hint }: { label: string; hint: string }) {
  return (
    <div style={{
      display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
      height: '100%', gap: 8,
    }}>
      <div style={{ fontSize: 14, fontWeight: 500, color: '#5f5e5a' }}>{label}</div>
      <div style={{ fontSize: 12, color: '#b4b2a9' }}>{hint}</div>
    </div>
  )
}
