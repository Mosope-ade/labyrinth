'use client'

import { useState } from 'react'
import { useReactFlow } from '@xyflow/react'

type Tool = 'cursor' | 'pan'

interface ToolButtonProps {
  active?: boolean
  onClick: () => void
  children: React.ReactNode
  title: string
}

function ToolBtn({ active, onClick, children, title }: ToolButtonProps) {
  const [hovered, setHovered] = useState(false)
  return (
    <button
      title={title}
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        width: 28, height: 28, borderRadius: 6,
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        border: 'none', cursor: 'pointer',
        background: active ? '#edf0ff' : hovered ? '#f0ede6' : 'transparent',
        color: active ? '#4340b0' : hovered ? '#1a1a18' : '#888780',
        transition: 'all 0.1s ease',
      }}
    >
      {children}
    </button>
  )
}

function Sep() {
  return <div style={{ width: 1, height: 16, background: '#e8e6e0', margin: '0 4px' }} />
}

export default function CanvasToolbar() {
  const [activeTool, setActiveTool] = useState<Tool>('cursor')
  const rf = useReactFlow()

  return (
    <div style={{
      height: 38, flexShrink: 0,
      background: '#ffffff',
      borderBottom: '1px solid #e8e6e0',
      display: 'flex', alignItems: 'center',
      padding: '0 10px',
      gap: 2,
    }}>
      {/* Left tools */}
      <ToolBtn active={activeTool === 'cursor'} onClick={() => setActiveTool('cursor')} title="Select">
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <path d="M2 2L6.5 12L8 8L12 6.5Z" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"/>
        </svg>
      </ToolBtn>

      <ToolBtn active={activeTool === 'pan'} onClick={() => setActiveTool('pan')} title="Pan">
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
          <circle cx="7" cy="7" r="5" stroke="currentColor" strokeWidth="1.2"/>
          <path d="M7 4V3M7 11V10M4 7H3M11 7H10" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/>
        </svg>
      </ToolBtn>

      <Sep />

      {/* Right — zoom controls */}
      <div style={{ flex: 1 }} />

      <ToolBtn onClick={() => rf.zoomOut()} title="Zoom out">
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
          <line x1="3" y1="6" x2="9" y2="6" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
        </svg>
      </ToolBtn>

      <span style={{ fontSize: 11, color: '#888780', minWidth: 36, textAlign: 'center' }}>
        100%
      </span>

      <ToolBtn onClick={() => rf.zoomIn()} title="Zoom in">
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
          <line x1="6" y1="3" x2="6" y2="9" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
          <line x1="3" y1="6" x2="9" y2="6" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
        </svg>
      </ToolBtn>

      <Sep />

      <ToolBtn onClick={() => rf.fitView({ padding: 0.15, duration: 300 })} title="Fit to screen">
        <svg width="13" height="13" viewBox="0 0 13 13" fill="none">
          <path d="M2 5V2H5M8 2H11V5M11 8V11H8M5 11H2V8" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/>
        </svg>
      </ToolBtn>
    </div>
  )
}
