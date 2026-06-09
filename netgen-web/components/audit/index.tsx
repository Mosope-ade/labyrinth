'use client'

import { useState, useCallback } from 'react'
import { useApp } from '@/lib/store'
import { sendChatMessage } from '@/lib/api'

interface AuditFlag {
  severity: 'critical' | 'warning' | 'info'
  component: string
  message: string
}

interface AuditReport {
  summary: string
  flags: AuditFlag[]
  suggestions: string[]
  compliance_notes: string[]
  raw: string
}

const SEVERITY_STYLE: Record<AuditFlag['severity'], string> = {
  critical: 'text-red-400 bg-red-400/10 border-red-400/30',
  warning:  'text-amber-400 bg-amber-400/10 border-amber-400/30',
  info:     'text-blue-400 bg-blue-400/10 border-blue-400/30',
}

const SEVERITY_ICON: Record<AuditFlag['severity'], string> = {
  critical: '✕',
  warning:  '⚠',
  info:     'ℹ',
}

function parseAuditReport(raw: string): AuditReport {
  // Best-effort structured parse — AI returns free text, we extract signals
  const flags: AuditFlag[] = []
  const suggestions: string[] = []
  const compliance_notes: string[] = []

  const lines = raw.split('\n')
  let summary = ''

  for (const line of lines) {
    const trimmed = line.trim()
    if (!trimmed) continue

    const critMatch = trimmed.match(/^\[CRITICAL\]\s*(.+)/i)
    const warnMatch = trimmed.match(/^\[WARN(?:ING)?\]\s*(.+)/i)
    const infoMatch = trimmed.match(/^\[INFO\]\s*(.+)/i)
    const suggMatch = trimmed.match(/^(?:Suggestion|Recommend):\s*(.+)/i)
    const compMatch = trimmed.match(/^(?:Compliance|Note):\s*(.+)/i)

    if (critMatch) {
      flags.push({ severity: 'critical', component: '', message: critMatch[1] })
    } else if (warnMatch) {
      flags.push({ severity: 'warning', component: '', message: warnMatch[1] })
    } else if (infoMatch) {
      flags.push({ severity: 'info', component: '', message: infoMatch[1] })
    } else if (suggMatch) {
      suggestions.push(suggMatch[1])
    } else if (compMatch) {
      compliance_notes.push(compMatch[1])
    } else if (!summary && trimmed.length > 20) {
      summary = trimmed
    }
  }

  if (!summary) summary = raw.split('\n').find((l) => l.trim().length > 20) ?? 'Audit complete.'

  return { summary, flags, suggestions, compliance_notes, raw }
}

export default function Audit() {
  const { state } = useApp()
  const graph = state.graph
  const [loading, setLoading] = useState(false)
  const [report, setReport] = useState<AuditReport | null>(null)
  const [showRaw, setShowRaw] = useState(false)

  const runAudit = useCallback(async () => {
    if (!graph) return
    setLoading(true)
    setReport(null)
    try {
      const nodeCount = graph.nodes.length
      const zoneCount = graph.zones.length
      const prompt =
        `Audit this network design: ${nodeCount} devices across ${zoneCount} security zones. ` +
        `Zones: ${graph.zones.map((z) => z.label).join(', ')}. ` +
        `Policies: ${graph.policies.length} zone policies. ` +
        `Please identify security risks, missing hardening, compliance concerns, and suggest improvements. ` +
        `Format critical issues as [CRITICAL] ..., warnings as [WARNING] ..., info as [INFO] ..., ` +
        `recommendations as Suggestion: ..., compliance as Compliance: ...`
      const res = await sendChatMessage(prompt, state.sessionId)
      setReport(parseAuditReport(res.reply))
    } catch (e) {
      setReport({
        summary: `Audit failed: ${e instanceof Error ? e.message : 'Unknown error'}`,
        flags: [],
        suggestions: [],
        compliance_notes: [],
        raw: '',
      })
    } finally {
      setLoading(false)
    }
  }, [graph, state.sessionId])

  if (!graph) {
    return (
      <div className="flex-1 bg-gray-950 flex items-center justify-center">
        <p className="text-gray-600 text-sm">No design loaded — build a network first</p>
      </div>
    )
  }

  const criticals = report?.flags.filter((f) => f.severity === 'critical').length ?? 0
  const warnings  = report?.flags.filter((f) => f.severity === 'warning').length ?? 0

  return (
    <div className="flex flex-col h-full bg-gray-950 text-sm">
      {/* Header */}
      <div className="px-4 py-3 border-b border-gray-800 flex items-center gap-3 flex-shrink-0">
        <button
          onClick={runAudit}
          disabled={loading}
          className="px-4 py-1.5 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 rounded text-xs font-medium text-white transition-colors"
        >
          {loading ? 'Auditing…' : report ? 'Re-run Audit' : 'Run AI Audit'}
        </button>
        {report && !loading && (
          <div className="flex items-center gap-3 text-xs">
            {criticals > 0 && (
              <span className="text-red-400 font-medium">{criticals} critical</span>
            )}
            {warnings > 0 && (
              <span className="text-amber-400 font-medium">{warnings} warnings</span>
            )}
            {criticals === 0 && warnings === 0 && (
              <span className="text-emerald-400 font-medium">No issues found</span>
            )}
          </div>
        )}
        {report && (
          <button
            onClick={() => setShowRaw((v) => !v)}
            className="ml-auto text-xs text-gray-600 hover:text-gray-400 transition-colors"
          >
            {showRaw ? 'Structured' : 'Raw'}
          </button>
        )}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto min-h-0 p-4 space-y-4">
        {!report && !loading && (
          <div className="h-full flex flex-col items-center justify-center text-center gap-3 py-8">
            <div className="text-gray-700 text-3xl">⬧</div>
            <p className="text-gray-600 text-xs max-w-xs leading-relaxed">
              AI audits your finalized design for security gaps, compliance issues, and design improvements.
              This is optional and does not block export.
            </p>
          </div>
        )}

        {loading && (
          <div className="flex items-center gap-3 text-gray-500 text-xs">
            <span className="flex gap-1">
              {[0, 150, 300].map((d) => (
                <span
                  key={d}
                  className="w-1.5 h-1.5 bg-gray-600 rounded-full animate-bounce"
                  style={{ animationDelay: `${d}ms` }}
                />
              ))}
            </span>
            Analysing design…
          </div>
        )}

        {report && !showRaw && (
          <>
            {/* Summary */}
            <div className="p-3 bg-gray-900 border border-gray-800 rounded-lg text-xs text-gray-300 leading-relaxed">
              {report.summary}
            </div>

            {/* Flags */}
            {report.flags.length > 0 && (
              <div className="space-y-2">
                <h3 className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Findings</h3>
                {report.flags.map((flag, i) => (
                  <div key={i} className={`flex gap-2 p-2.5 rounded-lg border text-xs ${SEVERITY_STYLE[flag.severity]}`}>
                    <span className="flex-shrink-0 font-bold">{SEVERITY_ICON[flag.severity]}</span>
                    <span className="leading-relaxed">{flag.message}</span>
                  </div>
                ))}
              </div>
            )}

            {/* Suggestions */}
            {report.suggestions.length > 0 && (
              <div className="space-y-2">
                <h3 className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Recommendations</h3>
                <ul className="space-y-1.5">
                  {report.suggestions.map((s, i) => (
                    <li key={i} className="flex gap-2 text-xs text-gray-300">
                      <span className="text-blue-500 flex-shrink-0">→</span>
                      {s}
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {/* Compliance */}
            {report.compliance_notes.length > 0 && (
              <div className="space-y-2">
                <h3 className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Compliance</h3>
                <ul className="space-y-1.5">
                  {report.compliance_notes.map((c, i) => (
                    <li key={i} className="flex gap-2 text-xs text-gray-400">
                      <span className="text-emerald-500 flex-shrink-0">✓</span>
                      {c}
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {/* Fallback if parse yielded nothing useful */}
            {report.flags.length === 0 && report.suggestions.length === 0 && report.compliance_notes.length === 0 && (
              <div className="p-3 bg-gray-900 border border-gray-800 rounded-lg text-xs text-gray-400 whitespace-pre-wrap leading-relaxed">
                {report.raw}
              </div>
            )}
          </>
        )}

        {report && showRaw && (
          <pre className="text-xs text-gray-400 font-mono whitespace-pre-wrap leading-relaxed bg-gray-900 border border-gray-800 rounded-lg p-3">
            {report.raw}
          </pre>
        )}
      </div>
    </div>
  )
}
