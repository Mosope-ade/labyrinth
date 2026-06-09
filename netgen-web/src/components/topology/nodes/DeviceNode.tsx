'use client'

import { memo } from 'react'
import { Handle, Position, type NodeProps } from '@xyflow/react'
import { NODE_ICON_REGISTRY } from '@/src/components/topology/icons'
import type { NodeRole } from '@/src/components/topology/icons'

export interface DeviceNodeData {
  label: string
  role: NodeRole
  ip?: string
  selected?: boolean
  dimmed?: boolean
  [key: string]: unknown
}

function DeviceNode({ data, selected }: NodeProps) {
  const d = data as DeviceNodeData
  const Icon = NODE_ICON_REGISTRY[d.role] ?? NODE_ICON_REGISTRY['Router']

  return (
    <div style={{
      display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4,
      cursor: 'pointer',
    }}>
      <Handle type="target" position={Position.Top} style={{ opacity: 0, width: 1, height: 1 }} />

      <Icon size={42} selected={!!selected} dimmed={!!d.dimmed} />

      <span style={{
        fontSize: 9, fontWeight: 500, color: '#5f5e5a',
        textAlign: 'center', maxWidth: 80,
        overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
      }}>
        {d.label}
      </span>

      {d.ip && (
        <span style={{
          fontSize: 8, fontFamily: 'var(--font-mono), monospace',
          color: '#888780',
        }}>
          {d.ip}
        </span>
      )}

      <Handle type="source" position={Position.Bottom} style={{ opacity: 0, width: 1, height: 1 }} />
    </div>
  )
}

export default memo(DeviceNode)
