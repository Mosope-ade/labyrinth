'use client'

import { useApp } from '@/lib/store'

const PIPELINE_STEPS = [
  'rule_engine',
  'security_policy',
  'addressing',
  'config_templates',
  'verification',
  'gns3_export',
]

const STEP_LABELS: Record<string, string> = {
  rule_engine:      'Rule engine',
  security_policy:  'Security policy',
  addressing:       'Addressing',
  config_templates: 'Config templates',
  verification:     'Verification',
  gns3_export:      'GNS3 export',
}

export default function BuildTimeline() {
  const { state } = useApp()

  if (!state.jobProgress && state.project?.status !== 'generating') return null

  const currentStep = state.jobProgress?.step ?? ''
  const currentIdx = PIPELINE_STEPS.indexOf(currentStep)

  return (
    <div style={{
      position: 'absolute', top: 12, right: 12,
      width: 158,
      background: '#ffffff',
      border: '1px solid #e8e6e0',
      borderRadius: 9,
      padding: '10px 12px',
      zIndex: 10,
      boxShadow: '0 1px 4px rgba(0,0,0,0.06)',
    }}>
      <div style={{
        fontSize: 10, fontWeight: 500, letterSpacing: '0.05em',
        textTransform: 'uppercase', color: '#888780',
        marginBottom: 8,
      }}>
        Building
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        {PIPELINE_STEPS.map((step, i) => {
          const done    = i < currentIdx
          const active  = i === currentIdx
          const pending = i > currentIdx

          return (
            <div key={step} style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
              {/* Dot */}
              <div style={{
                width: 14, height: 14, borderRadius: '50%', flexShrink: 0,
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                background: done ? '#1d9e75' : active ? '#4340b0' : '#f0ede6',
                border: pending ? '1px solid #e0ddd6' : 'none',
              }}>
                {done && (
                  <svg width="8" height="8" viewBox="0 0 8 8" fill="none">
                    <path d="M1.5 4L3.5 6L6.5 2.5" stroke="white" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round"/>
                  </svg>
                )}
                {active && (
                  <svg width="6" height="6" viewBox="0 0 6 6" fill="none">
                    <path d="M1.5 3L2.5 2L4.5 3.5" stroke="white" strokeWidth="1.2" strokeLinecap="round"/>
                  </svg>
                )}
              </div>

              {/* Label */}
              <span style={{
                fontSize: 10,
                fontWeight: active ? 600 : 400,
                color: done ? '#1d9e75' : active ? '#4340b0' : '#b4b2a9',
              }}>
                {STEP_LABELS[step] ?? step}
              </span>
            </div>
          )
        })}
      </div>
    </div>
  )
}
