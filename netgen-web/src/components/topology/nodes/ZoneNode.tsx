'use client'

import { memo } from 'react'
import type { NodeProps } from '@xyflow/react'
import { ZONE_COLOR_REGISTRY } from '@/src/components/topology/icons'
import type { ZoneRole } from '@/src/components/topology/icons'

export interface ZoneNodeData {
  name: string
  role: ZoneRole
  subnet?: string
  [key: string]: unknown
}

function ZoneNode({ data }: NodeProps) {
  const d = data as ZoneNodeData
  const colors = ZONE_COLOR_REGISTRY[d.role] ?? ZONE_COLOR_REGISTRY['Lan']

  return (
    <div style={{
      width: '100%', height: '100%',
      border: `1.5px dashed ${colors.border}`,
      background: colors.fill,
      borderRadius: 10,
      position: 'relative',
    }}>
      <div style={{
        position: 'absolute', top: -10, left: 10,
        background: colors.label.bg,
        color: colors.label.text,
        fontSize: 9, fontWeight: 600,
        padding: '1px 6px', borderRadius: 4,
        letterSpacing: '0.03em',
        whiteSpace: 'nowrap',
        border: `1px solid ${colors.border}22`,
      }}>
        {d.name.toUpperCase()}{d.subnet ? ` · ${d.subnet}` : ''}
      </div>
    </div>
  )
}

export default memo(ZoneNode)
