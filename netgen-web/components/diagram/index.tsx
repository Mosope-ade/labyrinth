'use client'

import { useState, useCallback } from 'react'
import DOMPurify from 'dompurify'
import { useApp } from '@/lib/store'
import { exportUrl, downloadExport } from '@/lib/api'

export default function Diagram() {
  const { state } = useApp()
  const project = state.project
  const [loading, setLoading] = useState(false)
  const [svgContent, setSvgContent] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    if (!project) return
    setLoading(true)
    setError(null)
    try {
      const url = exportUrl(project.id, 'svg')
      const res = await fetch(url, {
        headers: { Authorization: `Bearer ${localStorage.getItem('laby_token') ?? ''}` },
      })
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const text = await res.text()
      setSvgContent(DOMPurify.sanitize(text, { USE_PROFILES: { svg: true } }))
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load diagram')
    } finally {
      setLoading(false)
    }
  }, [project])

  const downloadPng = useCallback(async () => {
    if (!svgContent) return
    // Convert SVG → PNG via canvas
    const blob = new Blob([svgContent], { type: 'image/svg+xml' })
    const url = URL.createObjectURL(blob)
    const img = new Image()
    img.onload = () => {
      const canvas = document.createElement('canvas')
      canvas.width = img.width || 1200
      canvas.height = img.height || 800
      const ctx = canvas.getContext('2d')!
      ctx.fillStyle = '#030712'
      ctx.fillRect(0, 0, canvas.width, canvas.height)
      ctx.drawImage(img, 0, 0)
      canvas.toBlob((pngBlob) => {
        if (!pngBlob) return
        const a = document.createElement('a')
        a.href = URL.createObjectURL(pngBlob)
        a.download = `${state.project?.name ?? 'diagram'}.png`
        a.click()
      }, 'image/png')
      URL.revokeObjectURL(url)
    }
    img.src = url
  }, [svgContent, state.project])

  const downloadSvg = useCallback(() => {
    if (!project) return
    downloadExport(project.id, 'svg', `${project.name}.svg`)
  }, [project])

  if (!project) {
    return (
      <div className="flex-1 bg-gray-950 flex items-center justify-center">
        <p className="text-gray-600 text-sm">No project loaded</p>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full bg-gray-950">
      {/* Toolbar */}
      <div className="flex items-center gap-2 px-4 py-2 border-b border-gray-800 flex-shrink-0">
        <button
          onClick={load}
          disabled={loading}
          className="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 rounded text-xs font-medium text-white transition-colors"
        >
          {loading ? 'Loading…' : svgContent ? 'Refresh' : 'Load Diagram'}
        </button>
        {svgContent && (
          <>
            <button
              onClick={downloadSvg}
              className="px-3 py-1.5 bg-gray-800 hover:bg-gray-700 rounded text-xs font-medium text-gray-300 transition-colors"
            >
              ↓ SVG
            </button>
            <button
              onClick={downloadPng}
              className="px-3 py-1.5 bg-gray-800 hover:bg-gray-700 rounded text-xs font-medium text-gray-300 transition-colors"
            >
              ↓ PNG
            </button>
          </>
        )}
        {error && <span className="text-xs text-red-400 ml-2">{error}</span>}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-4 min-h-0">
        {!svgContent && !loading && (
          <div className="h-full flex items-center justify-center">
            <div className="text-center space-y-2">
              <div className="text-gray-700 text-4xl">◻</div>
              <p className="text-gray-600 text-sm">Click &ldquo;Load Diagram&rdquo; to render the architecture diagram</p>
            </div>
          </div>
        )}
        {svgContent && (
          <div
            className="flex justify-center"
            dangerouslySetInnerHTML={{ __html: svgContent }}
          />
        )}
      </div>
    </div>
  )
}
