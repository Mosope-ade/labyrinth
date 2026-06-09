'use client'

import { useState, useCallback } from 'react'
import { useApp } from '@/lib/store'
import { downloadExport } from '@/lib/api'
import type { NetworkNode } from '@/lib/types'

function ConfigViewer({ config }: { config: string }) {
  // Minimal syntax highlighting via regex — no external lib needed
  const lines = config.split('\n')

  function highlightLine(line: string, idx: number) {
    // Comments
    if (line.trimStart().startsWith('!')) {
      return (
        <span key={idx} className="text-gray-600">
          {line}
          {'\n'}
        </span>
      )
    }
    // Section headers (e.g. "interface", "router ospf")
    if (/^(interface|router|ip route|access-list|line |hostname|banner|crypto)/i.test(line.trim())) {
      return (
        <span key={idx} className="text-blue-400 font-semibold">
          {line}
          {'\n'}
        </span>
      )
    }
    // IP addresses
    if (/\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}/.test(line)) {
      const parts = line.split(/(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}(?:\/\d{1,2})?)/)
      return (
        <span key={idx}>
          {parts.map((p, pi) =>
            /\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}/.test(p) ? (
              <span key={pi} className="text-emerald-400">
                {p}
              </span>
            ) : (
              <span key={pi} className="text-gray-300">
                {p}
              </span>
            )
          )}
          {'\n'}
        </span>
      )
    }
    // "no" commands
    if (/^\s+no /.test(line)) {
      return (
        <span key={idx} className="text-red-400/80">
          {line}
          {'\n'}
        </span>
      )
    }
    return (
      <span key={idx} className="text-gray-300">
        {line}
        {'\n'}
      </span>
    )
  }

  return (
    <pre className="text-xs leading-relaxed font-mono overflow-x-auto">
      {lines.map((line, i) => highlightLine(line, i))}
    </pre>
  )
}

export default function Configs() {
  const { state } = useApp()
  const graph = state.graph
  const project = state.project
  const [selectedId, setSelectedId] = useState<string | null>(null)

  const nodesWithConfig: NetworkNode[] = graph?.nodes.filter((n) => n.config) ?? []

  const selectedNode = nodesWithConfig.find((n) => n.id === selectedId) ?? nodesWithConfig[0] ?? null

  const downloadAll = useCallback(() => {
    if (!project) return
    downloadExport(project.id, 'gns3', `${project.name}-configs.zip`)
  }, [project])

  const downloadOne = useCallback((node: NetworkNode) => {
    if (!node.config) return
    const blob = new Blob([node.config.raw], { type: 'text/plain' })
    const a = document.createElement('a')
    a.href = URL.createObjectURL(blob)
    a.download = `${node.label}.cfg`
    a.click()
  }, [])

  if (!graph || nodesWithConfig.length === 0) {
    return (
      <div className="flex-1 bg-gray-950 flex items-center justify-center">
        <p className="text-gray-600 text-sm">
          {graph ? 'No configs generated yet' : 'No project loaded'}
        </p>
      </div>
    )
  }

  return (
    <div className="flex h-full bg-gray-950 text-sm min-h-0">
      {/* Device list */}
      <div className="w-48 flex-shrink-0 border-r border-gray-800 flex flex-col">
        <div className="px-3 py-2 border-b border-gray-800 flex items-center justify-between">
          <span className="text-xs font-semibold text-gray-400 uppercase tracking-wider">Devices</span>
          {project && (
            <button
              onClick={downloadAll}
              title="Download GNS3 export (all configs)"
              className="text-xs text-blue-400 hover:text-blue-300 transition-colors"
            >
              ↓ All
            </button>
          )}
        </div>
        <div className="flex-1 overflow-y-auto py-1">
          {nodesWithConfig.map((n) => (
            <button
              key={n.id}
              onClick={() => setSelectedId(n.id)}
              className={`w-full text-left px-3 py-2 text-xs transition-colors flex items-center gap-2 ${
                n.id === (selectedNode?.id ?? null)
                  ? 'bg-blue-600/20 text-blue-300'
                  : 'text-gray-400 hover:text-gray-200 hover:bg-gray-900'
              }`}
            >
              <span className="truncate font-mono">{n.label}</span>
            </button>
          ))}
        </div>
      </div>

      {/* Config viewer */}
      {selectedNode?.config ? (
        <div className="flex-1 flex flex-col min-w-0">
          <div className="px-4 py-2 border-b border-gray-800 flex items-center justify-between flex-shrink-0">
            <span className="text-xs font-mono text-gray-300">{selectedNode.label}.cfg</span>
            <button
              onClick={() => downloadOne(selectedNode)}
              className="text-xs text-blue-400 hover:text-blue-300 transition-colors"
            >
              ↓ Download
            </button>
          </div>
          <div className="flex-1 overflow-auto p-4 bg-gray-950 min-h-0">
            <ConfigViewer config={selectedNode.config.raw} />
          </div>
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center text-gray-600 text-sm">
          Select a device
        </div>
      )}
    </div>
  )
}
