'use client'

import { useRef, useState, useEffect, KeyboardEvent } from 'react'
import { useApp } from '@/lib/store'
import { sendChatMessage, getGraphForSession, isTauri, ApiError } from '@/lib/api'
import type { DesignPhase, Industry } from '@/lib/types'

const PHASE_LABEL: Record<DesignPhase, string> = {
  gathering: 'Gathering',
  building:  'Building',
  refining:  'Refining',
}
const PHASE_COLOR: Record<DesignPhase, string> = {
  gathering: 'bg-blue-500/20 text-blue-300 border-blue-500/30',
  building:  'bg-amber-500/20 text-amber-300 border-amber-500/30',
  refining:  'bg-emerald-500/20 text-emerald-300 border-emerald-500/30',
}
const INDUSTRY_LABEL: Record<Industry, string> = {
  healthcare: '🏥 Healthcare',
  retail:     '🛒 Retail',
  education:  '🎓 Education',
  industrial: '🏭 Industrial',
  enterprise: '🏢 Enterprise',
}

export default function Chat() {
  const { state, dispatch } = useApp()
  const [input, setInput] = useState('')
  const bottomRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [state.messages, state.isThinking])

  async function submit() {
    const text = input.trim()
    if (!text || state.isThinking) return
    setInput('')
    dispatch({ type: 'CHAT_USER_MSG', content: text })
    dispatch({ type: 'CHAT_THINKING' })
    try {
      const res = await sendChatMessage(text, state.sessionId, state.projectId)
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
      // In desktop mode the pipeline runs inline — fetch the graph immediately
      if (res.intent_ready && isTauri) {
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

  function onKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      submit()
    }
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="px-4 py-3 border-b border-gray-800 flex items-center justify-between gap-2 flex-shrink-0">
        <span className="text-xs font-semibold text-gray-400 uppercase tracking-wider">Design</span>
        <div className="flex items-center gap-2">
          {state.detectedIndustry && (
            <span className="text-xs text-gray-500">{INDUSTRY_LABEL[state.detectedIndustry]}</span>
          )}
          <span className={`text-xs px-2 py-0.5 rounded-full border font-medium ${PHASE_COLOR[state.phase]}`}>
            {PHASE_LABEL[state.phase]}
          </span>
        </div>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-4 py-3 space-y-4 text-sm min-h-0">
        {state.messages.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full text-center gap-3 py-8">
            <div className="text-xl font-bold text-gray-300">Labyrinth</div>
            <p className="text-gray-500 max-w-xs leading-relaxed text-xs">
              Describe your network. I&apos;ll design it, verify it, and generate configs and a GNS3 lab.
            </p>
            <div className="mt-2 space-y-2 text-left w-full">
              {[
                '3-site OSPF hospital network with HIPAA compliance',
                'Small office with guest wifi and DMZ for web servers',
                'Branch network with redundancy back to HQ',
              ].map((ex) => (
                <button
                  key={ex}
                  onClick={() => setInput(ex)}
                  className="w-full text-left px-3 py-2 rounded-lg bg-gray-900 border border-gray-800 text-gray-400 hover:text-gray-200 hover:border-gray-700 transition-colors text-xs"
                >
                  &ldquo;{ex}&rdquo;
                </button>
              ))}
            </div>
          </div>
        )}

        {state.messages.map((msg, i) => (
          <div key={i} className={`flex gap-2 ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}>
            {msg.role === 'assistant' && (
              <div className="w-6 h-6 rounded-full bg-blue-600 flex-shrink-0 flex items-center justify-center text-xs font-bold text-white mt-0.5">L</div>
            )}
            <div className={`max-w-[85%] rounded-xl px-3 py-2 text-sm leading-relaxed whitespace-pre-wrap ${
              msg.role === 'user'
                ? 'bg-blue-600 text-white rounded-tr-sm'
                : 'bg-gray-900 text-gray-200 rounded-tl-sm border border-gray-800'
            }`}>
              {msg.content}
            </div>
          </div>
        ))}

        {state.isThinking && (
          <div className="flex gap-2 justify-start">
            <div className="w-6 h-6 rounded-full bg-blue-600 flex-shrink-0 flex items-center justify-center text-xs font-bold text-white">L</div>
            <div className="bg-gray-900 border border-gray-800 rounded-xl rounded-tl-sm px-3 py-2.5">
              <span className="flex gap-1 items-center">
                {[0, 150, 300].map((delay) => (
                  <span key={delay} className="w-1.5 h-1.5 bg-gray-500 rounded-full animate-bounce" style={{ animationDelay: `${delay}ms` }} />
                ))}
              </span>
            </div>
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      {/* Progress bar */}
      {state.jobProgress && (
        <div className="px-4 py-2 border-t border-gray-800 bg-gray-950 flex-shrink-0">
          <div className="flex justify-between text-xs text-gray-500 mb-1">
            <span className="capitalize">{state.jobProgress.step.replace(/_/g, ' ')}</span>
            <span>{state.jobProgress.percent}%</span>
          </div>
          <div className="h-1 bg-gray-800 rounded-full overflow-hidden">
            <div className="h-full bg-blue-500 rounded-full transition-all duration-300" style={{ width: `${state.jobProgress.percent}%` }} />
          </div>
        </div>
      )}

      {/* Input */}
      <div className="px-4 py-3 border-t border-gray-800 flex-shrink-0">
        {state.fallbackRequired && (
          <div className="mb-2 px-3 py-2 bg-amber-900/30 border border-amber-800/50 rounded-lg text-xs text-amber-300">
            Couldn&apos;t parse a complete intent. Try rephrasing or use the form builder.
          </div>
        )}
        <div className="flex gap-2 items-end">
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder={state.phase === 'building' ? 'Building your network…' : state.phase === 'refining' ? 'Refine your design…' : 'Describe your network…'}
            disabled={state.isThinking || state.phase === 'building'}
            rows={1}
            className="flex-1 bg-gray-900 border border-gray-700 rounded-lg px-3 py-2 text-sm text-gray-100 placeholder-gray-600 focus:outline-none focus:border-blue-500 disabled:opacity-50 resize-none leading-5 overflow-y-auto"
            style={{ minHeight: '2.5rem', maxHeight: '8rem' }}
          />
          <button
            onClick={submit}
            disabled={!input.trim() || state.isThinking || state.phase === 'building'}
            className="px-3 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-40 disabled:cursor-not-allowed rounded-lg text-sm font-medium text-white transition-colors flex-shrink-0"
          >
            Send
          </button>
        </div>
        <p className="mt-1.5 text-xs text-gray-700">Enter to send · Shift+Enter for newline</p>
      </div>
    </div>
  )
}
