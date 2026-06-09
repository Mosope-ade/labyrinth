// API client — switches between HTTP (SaaS) and Tauri IPC (desktop).
// The rest of the frontend never imports fetch or Tauri directly.

import type {
  AuthResponse,
  ChatTurnResponse,
  ConversationTurn,
  LoginResponse,
  Project,
  ProjectDetail,
  TopologyGraph,
} from './types'

export const isTauri =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

const BASE = process.env.NEXT_PUBLIC_API_URL ?? 'http://localhost:8080'

// ─── HTTP helpers ─────────────────────────────────────────────────────────────

function getToken(): string | null {
  if (typeof window === 'undefined') return null
  return localStorage.getItem('laby_token')
}

async function apiFetch<T>(
  path: string,
  init: RequestInit = {},
): Promise<T> {
  const token = getToken()
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(init.headers as Record<string, string> | undefined),
  }
  if (token) headers['Authorization'] = `Bearer ${token}`

  const res = await fetch(`${BASE}${path}`, { ...init, headers })
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    let message: string
    try {
      const body = JSON.parse(text)
      message = body.error ?? body.message ?? text
    } catch {
      message = text || res.statusText
    }
    throw new ApiError(res.status, message)
  }
  return res.json() as Promise<T>
}

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

// ─── Auth ─────────────────────────────────────────────────────────────────────

export async function register(
  email: string,
  password: string,
): Promise<AuthResponse> {
  return apiFetch('/api/auth/register', {
    method: 'POST',
    body: JSON.stringify({ email, password }),
  })
}

export async function login(
  email: string,
  password: string,
): Promise<LoginResponse> {
  return apiFetch('/api/auth/login', {
    method: 'POST',
    body: JSON.stringify({ email, password }),
  })
}

// ─── Chat / Intent ────────────────────────────────────────────────────────────

export async function sendChatMessage(
  message: string,
  sessionId: string | null,
  projectId: string | null = null,
): Promise<ChatTurnResponse> {
  if (isTauri) {
    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    // @ts-ignore — Tauri package is not installed in web build; guarded by isTauri check
    const { invoke } = await import(/* webpackIgnore: true */ '@tauri-apps/api/core')
    return invoke('chat_turn', { message, sessionId })
  }
  return apiFetch('/api/intent/chat', {
    method: 'POST',
    body: JSON.stringify({ message, session_id: sessionId, project_id: projectId }),
  })
}

// Desktop-only: fetch the compiled graph for a session after intent_ready
export async function getGraphForSession(
  sessionId: string,
): Promise<TopologyGraph | null> {
  if (!isTauri) return null
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore
  const { invoke } = await import(/* webpackIgnore: true */ '@tauri-apps/api/core')
  return invoke('get_graph', { sessionId })
}

// Desktop-only: save a project locally
export async function saveProjectLocally(
  name: string,
  graph: TopologyGraph,
): Promise<{ id: string; name: string; created_at: string }> {
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore
  const { invoke } = await import(/* webpackIgnore: true */ '@tauri-apps/api/core')
  return invoke('save_project', { name, graphJson: JSON.stringify(graph) })
}

// Desktop-only: get export content as a string
export async function getExportContent(
  sessionId: string,
  format: 'svg' | 'csv' | 'md',
): Promise<string> {
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore
  const { invoke } = await import(/* webpackIgnore: true */ '@tauri-apps/api/core')
  return invoke('export_content', { sessionId, format })
}

// ─── Projects ────────────────────────────────────────────────────────────────

export async function createProject(name: string): Promise<Project> {
  return apiFetch('/api/projects', {
    method: 'POST',
    body: JSON.stringify({ name }),
  })
}

export async function listProjects(): Promise<Project[]> {
  return apiFetch('/api/projects')
}

export async function getProject(id: string): Promise<ProjectDetail> {
  return apiFetch(`/api/projects/${id}`)
}

export async function getProjectTurns(id: string): Promise<ConversationTurn[]> {
  return apiFetch(`/api/projects/${id}/turns`)
}

export async function deleteProject(id: string): Promise<void> {
  await apiFetch(`/api/projects/${id}`, { method: 'DELETE' })
}

// ─── Exports ──────────────────────────────────────────────────────────────────

export function exportUrl(projectId: string, format: 'gns3' | 'svg' | 'csv' | 'docs'): string {
  return `${BASE}/api/projects/${projectId}/export/${format}`
}

export async function downloadExport(
  projectId: string,
  format: 'gns3' | 'svg' | 'csv' | 'docs',
  filename: string,
): Promise<void> {
  const token = getToken()
  const headers: Record<string, string> = {}
  if (token) headers['Authorization'] = `Bearer ${token}`

  const res = await fetch(`${BASE}/api/projects/${projectId}/export/${format}`, { headers })
  if (!res.ok) throw new ApiError(res.status, `Export failed: ${res.statusText}`)

  const blob = await res.blob()
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

// ─── WebSocket ────────────────────────────────────────────────────────────────

export function connectJobStream(
  projectId: string,
  onEvent: (e: import('./types').JobEvent) => void,
  onClose?: () => void,
): WebSocket {
  const wsBase = BASE.replace(/^http/, 'ws')
  const token = getToken()
  const query = token ? `?token=${encodeURIComponent(token)}` : ''
  const ws = new WebSocket(`${wsBase}/api/projects/${projectId}/stream${query}`)

  ws.onmessage = (ev) => {
    try {
      onEvent(JSON.parse(ev.data))
    } catch {
      // malformed frame — ignore
    }
  }

  ws.onclose = () => onClose?.()
  return ws
}
