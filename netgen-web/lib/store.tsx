'use client'

// Global app state via React context + useReducer.
// All components read from here — single source of truth in the frontend.

import React, { createContext, useContext, useReducer } from 'react'
import type { ChatMessage, ConversationTurn, DesignPhase, Industry, JobEvent, Project, TopologyGraph } from './types'

// ─── State shape ─────────────────────────────────────────────────────────────

export type ActiveTab = 'topology' | 'diagram' | 'configs' | 'ip-plan' | 'simulation' | 'audit'

export interface AppState {
  // Auth
  token: string | null
  userEmail: string | null

  // AI conversation
  sessionId: string | null
  phase: DesignPhase
  messages: ChatMessage[]
  detectedIndustry: Industry | null
  fallbackRequired: boolean
  isThinking: boolean

  // Project
  project: Project | null
  projectId: string | null
  graph: TopologyGraph | null

  // Project history
  projects: Project[]

  // Job progress
  jobProgress: { step: string; percent: number } | null

  // UI
  activeTab: ActiveTab
  selectedNodeId: string | null
}

const initial: AppState = {
  token: null,
  userEmail: null,
  sessionId: null,
  phase: 'gathering',
  messages: [],
  detectedIndustry: null,
  fallbackRequired: false,
  isThinking: false,
  project: null,
  projectId: null,
  graph: null,
  projects: [],
  jobProgress: null,
  activeTab: 'topology',
  selectedNodeId: null,
}

// ─── Actions ──────────────────────────────────────────────────────────────────

export type Action =
  | { type: 'AUTH_SET'; token: string; email: string }
  | { type: 'AUTH_CLEAR' }
  | { type: 'CHAT_THINKING' }
  | { type: 'CHAT_USER_MSG'; content: string }
  | {
      type: 'CHAT_REPLY'
      sessionId: string
      phase: DesignPhase
      reply: string
      intentReady: boolean
      fallbackRequired: boolean
      industry: Industry | null
      projectId: string | null
    }
  | { type: 'CHAT_ERROR'; message: string }
  | { type: 'PROJECT_SET'; project: Project }
  | { type: 'PROJECTS_SET'; projects: Project[] }
  | {
      type: 'PROJECT_LOAD'
      project: Project
      graph: TopologyGraph | null
      turns: ConversationTurn[]
    }
  | { type: 'GRAPH_SET'; graph: TopologyGraph }
  | { type: 'JOB_EVENT'; event: JobEvent }
  | { type: 'TAB_SET'; tab: ActiveTab }
  | { type: 'NODE_SELECT'; id: string | null }

// ─── Reducer ──────────────────────────────────────────────────────────────────

function reducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case 'AUTH_SET':
      return { ...state, token: action.token, userEmail: action.email }

    case 'AUTH_CLEAR':
      return { ...initial }

    case 'CHAT_THINKING':
      return { ...state, isThinking: true }

    case 'CHAT_USER_MSG':
      return {
        ...state,
        messages: [...state.messages, { role: 'user', content: action.content }],
      }

    case 'CHAT_REPLY': {
      const newState: AppState = {
        ...state,
        isThinking: false,
        sessionId: action.sessionId,
        phase: action.phase,
        fallbackRequired: action.fallbackRequired,
        detectedIndustry: action.industry ?? state.detectedIndustry,
        messages: [
          ...state.messages,
          { role: 'assistant', content: action.reply },
        ],
        activeTab: action.intentReady ? 'topology' : state.activeTab,
      }
      if (action.projectId && !state.projectId) {
        newState.projectId = action.projectId
      }
      return newState
    }

    case 'CHAT_ERROR':
      return {
        ...state,
        isThinking: false,
        messages: [...state.messages, { role: 'assistant', content: action.message }],
      }

    case 'PROJECT_SET':
      return { ...state, project: action.project }

    case 'PROJECTS_SET':
      return { ...state, projects: action.projects }

    case 'PROJECT_LOAD':
      return {
        ...state,
        project: action.project,
        projectId: action.project.id,
        graph: action.graph,
        // Restore the conversation history as read-only messages
        messages: action.turns.map(t => ({ role: t.role, content: t.content })),
        phase: 'refining',
        sessionId: null,
        isThinking: false,
        fallbackRequired: false,
        activeTab: 'topology',
      }

    case 'GRAPH_SET':
      return { ...state, graph: action.graph }

    case 'JOB_EVENT':
      if (action.event.type === 'progress') {
        return {
          ...state,
          jobProgress: { step: action.event.step, percent: action.event.percent },
        }
      }
      if (action.event.type === 'complete') {
        return { ...state, jobProgress: null }
      }
      if (action.event.type === 'failed') {
        return { ...state, jobProgress: null }
      }
      return state

    case 'TAB_SET':
      return { ...state, activeTab: action.tab }

    case 'NODE_SELECT':
      return { ...state, selectedNodeId: action.id }

    default:
      return state
  }
}

// ─── Context ──────────────────────────────────────────────────────────────────

const Ctx = createContext<{
  state: AppState
  dispatch: React.Dispatch<Action>
} | null>(null)

export function AppProvider({ children }: { children: React.ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initial, (s) => {
    // Rehydrate token from localStorage on first render
    if (typeof window === 'undefined') return s
    const token = localStorage.getItem('laby_token')
    const email = localStorage.getItem('laby_email')
    return token && email ? { ...s, token, userEmail: email } : s
  })

  return <Ctx.Provider value={{ state, dispatch }}>{children}</Ctx.Provider>
}

export function useApp() {
  const ctx = useContext(Ctx)
  if (!ctx) throw new Error('useApp must be used within AppProvider')
  return ctx
}
