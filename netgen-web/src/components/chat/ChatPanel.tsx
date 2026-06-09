'use client'

import { useRef, useEffect, useState } from 'react'
import { useApp } from '@/lib/store'
import { sendChatMessage, getGraphForSession, ApiError } from '@/lib/api'

// ─── Code block renderer ──────────────────────────────────────────────────────

function CodeBlock({ code }: { code: string }) {
  return (
    <pre style={{
      background: '#1a1a18', color: '#9fe1cb',
      fontSize: 10, fontFamily: 'var(--font-mono), monospace',
      padding: '8px 10px', borderRadius: 6,
      overflow: 'auto', whiteSpace: 'pre-wrap',
      margin: '6px 0 0 0',
    }}>
      {code}
    </pre>
  )
}

// ─── Message bubble ───────────────────────────────────────────────────────────

function MessageBubble({ role, content }: { role: 'user' | 'assistant'; content: string }) {
  const isUser = role === 'user'

  // Split content into text and code blocks
  const parts = content.split(/(```[\s\S]*?```)/g)

  return (
    <div style={{
      display: 'flex',
      justifyContent: isUser ? 'flex-end' : 'flex-start',
      padding: '0 12px',
    }}>
      <div style={{
        maxWidth: '88%',
        background: isUser ? '#1a1a18' : '#f5f4f1',
        color: isUser ? '#ffffff' : '#1a1a18',
        borderRadius: isUser ? '10px 10px 2px 10px' : '2px 10px 10px 10px',
        padding: '8px 10px',
        fontSize: 12, lineHeight: 1.5,
      }}>
        {parts.map((part, i) => {
          if (part.startsWith('```')) {
            const code = part.replace(/^```\w*\n?/, '').replace(/```$/, '')
            return <CodeBlock key={i} code={code} />
          }
          return <span key={i} style={{ whiteSpace: 'pre-wrap' }}>{part}</span>
        })}
      </div>
    </div>
  )
}

// ─── Thinking indicator ───────────────────────────────────────────────────────

function ThinkingIndicator() {
  return (
    <div style={{ display: 'flex', justifyContent: 'flex-start', padding: '0 12px' }}>
      <div style={{
        background: '#f5f4f1', borderRadius: '2px 10px 10px 10px',
        padding: '10px 14px', display: 'flex', alignItems: 'center', justifyContent: 'center',
      }}>
        <svg
          width="20" height="20" viewBox="0 0 22 22" fill="none"
          style={{ animation: 'logo-roll 1.8s linear infinite' }}
        >
          <path
            d="M11 2L19.5 7V15L11 20L2.5 15V7L11 2Z"
            fill="#1a1a18" stroke="#1a1a18" strokeWidth="1.2" strokeLinejoin="round"
          />
          <line x1="11" y1="2" x2="11" y2="20" stroke="#f5f4f1" strokeWidth="0.9"
            style={{ animation: 'grid-pulse 1.8s ease-in-out infinite' }} />
          <line x1="2.5" y1="7" x2="19.5" y2="7" stroke="#f5f4f1" strokeWidth="0.9"
            style={{ animation: 'grid-pulse 1.8s ease-in-out infinite 0.3s' }} />
          <line x1="2.5" y1="15" x2="19.5" y2="15" stroke="#f5f4f1" strokeWidth="0.9"
            style={{ animation: 'grid-pulse 1.8s ease-in-out infinite 0.6s' }} />
        </svg>
      </div>
    </div>
  )
}

// ─── Phase badge ──────────────────────────────────────────────────────────────

const PHASE_COLORS = {
  gathering: { bg: '#edf4fd', text: '#185fa5' },
  building:  { bg: '#edf0ff', text: '#4340b0' },
  refining:  { bg: '#edf7f2', text: '#0f6e56' },
}

// ─── Chat Panel ───────────────────────────────────────────────────────────────

export default function ChatPanel() {
  const { state, dispatch } = useApp()
  const [input, setInput] = useState('')
  const bottomRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [state.messages, state.isThinking])

  useEffect(() => {
    const el = inputRef.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 96)}px`
  }, [input])

  async function handleSend() {
    const msg = input.trim()
    if (!msg || state.isThinking) return

    setInput('')
    dispatch({ type: 'CHAT_USER_MSG', content: msg })
    dispatch({ type: 'CHAT_THINKING' })

    try {
      const res = await sendChatMessage(msg, state.sessionId, state.projectId)
      dispatch({
        type: 'CHAT_REPLY',
        sessionId: res.session_id,
        phase: res.phase,
        reply: res.reply,
        intentReady: res.intent_ready,
        fallbackRequired: res.fallback_required,
        industry: res.detected_industry,
        projectId: res.project_id,
      })

      if (res.intent_ready) {
        const graph = await getGraphForSession(res.session_id)
        if (graph) dispatch({ type: 'GRAPH_SET', graph })
      }
    } catch (err) {
      let message = "I'm having trouble responding right now. Please try again."
      if (err instanceof ApiError) {
        if (err.status === 429) message = "I'm a bit overloaded right now. Please wait a moment and try again."
        else if (err.status === 503) message = "The design engine is temporarily unavailable. Please try again shortly."
        else if (err.status >= 500) message = "Something went wrong on my end. Please try again."
        else if (err.status === 401 || err.status === 403) message = "I'm not authorized to respond right now. Please contact support."
      } else if (err instanceof TypeError) {
        message = "I couldn't reach the server. Check your connection and try again."
      }
      dispatch({ type: 'CHAT_ERROR', message })
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  const phaseColors = PHASE_COLORS[state.phase] ?? PHASE_COLORS.gathering

  return (
    <div style={{
      width: 300, flexShrink: 0,
      background: '#ffffff',
      borderLeft: '1px solid #e8e6e0',
      display: 'flex', flexDirection: 'column',
    }}>
      {/* Header */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 8,
        padding: '0 12px', height: 44,
        borderBottom: '1px solid #e8e6e0', flexShrink: 0,
      }}>
        <div style={{
          width: 24, height: 24, borderRadius: '50%',
          background: 'linear-gradient(135deg, #534AB7, #7F77DD)',
          display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0,
        }}>
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
            <circle cx="6" cy="3" r="1.5" fill="white"/>
            <circle cx="2" cy="9" r="1.5" fill="white"/>
            <circle cx="10" cy="9" r="1.5" fill="white"/>
            <line x1="6" y1="4.5" x2="2" y2="7.5" stroke="white" strokeWidth="1"/>
            <line x1="6" y1="4.5" x2="10" y2="7.5" stroke="white" strokeWidth="1"/>
          </svg>
        </div>
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 13, fontWeight: 600, color: '#1a1a18' }}>Labyrinth AI</div>
        </div>
        <div style={{
          fontSize: 9, fontWeight: 600, textTransform: 'uppercase', letterSpacing: '0.04em',
          background: phaseColors.bg, color: phaseColors.text,
          padding: '2px 6px', borderRadius: 4,
        }}>
          {state.phase}
        </div>
      </div>

      {/* Messages */}
      <div style={{ flex: 1, overflow: 'auto', padding: '12px 0', display: 'flex', flexDirection: 'column', gap: 8 }}>
        {state.messages.length === 0 && (
          <div style={{ padding: '0 12px' }}>
            <div style={{
              background: '#f5f4f1', borderRadius: '2px 10px 10px 10px',
              padding: '10px 12px', fontSize: 12, color: '#5f5e5a', lineHeight: 1.5,
            }}>
              Describe your network and I&apos;ll design it for you. Tell me about your sites,
              devices, security requirements, or industry — whatever you have.
            </div>
          </div>
        )}

        {state.messages.map((msg, i) => (
          <MessageBubble key={i} role={msg.role} content={msg.content} />
        ))}

        {state.isThinking && <ThinkingIndicator />}
        <div ref={bottomRef} />
      </div>

      {/* Input */}
      <div style={{
        padding: '10px 12px', borderTop: '1px solid #e8e6e0', flexShrink: 0,
      }}>
        <div style={{
          background: '#f5f4f1', border: '1px solid #e0ddd6',
          borderRadius: 10, padding: '8px 10px',
          display: 'flex', gap: 8, alignItems: 'flex-end',
        }}>
          <textarea
            ref={inputRef}
            value={input}
            onChange={e => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Ask a question…"
            disabled={state.isThinking}
            rows={1}
            style={{
              flex: 1, background: 'none', border: 'none', outline: 'none',
              fontSize: 12, color: '#1a1a18', resize: 'none',
              fontFamily: 'inherit', lineHeight: 1.5,
              height: 18, maxHeight: 96, overflow: 'auto',
              display: 'block',
            }}
          />
          <button
            onClick={handleSend}
            disabled={!input.trim() || state.isThinking}
            style={{
              width: 26, height: 26, borderRadius: 7, flexShrink: 0,
              background: input.trim() && !state.isThinking ? '#1a1a18' : '#e8e6e0',
              border: 'none', cursor: input.trim() ? 'pointer' : 'default',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              transition: 'background 0.1s ease',
            }}
          >
            <svg width="11" height="11" viewBox="0 0 11 11" fill="none">
              <path d="M5.5 9V2M2 5.5L5.5 2L9 5.5"
                stroke={input.trim() && !state.isThinking ? 'white' : '#b4b2a9'}
                strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </button>
        </div>
        <div style={{ marginTop: 5, fontSize: 10, color: '#b4b2a9', textAlign: 'center' }}>
          ↵ to send · shift+↵ new line
        </div>
      </div>

      <style>{`
        @keyframes logo-roll {
          from { transform: rotate(0deg); }
          to   { transform: rotate(360deg); }
        }
        @keyframes grid-pulse {
          0%, 100% { opacity: 0.2; }
          50%       { opacity: 0.7; }
        }
      `}</style>
    </div>
  )
}
