interface IconProps {
  size?: number
  selected?: boolean
  dimmed?: boolean
}

export function RouterIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const inner = Math.round(size * 0.65)
  return (
    <div style={{
      width: size, height: size,
      background: '#EBF4FD',
      border: `${selected ? 2 : 1.5}px solid ${selected ? '#185FA5' : '#378ADD'}`,
      borderRadius: 8,
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      opacity: dimmed ? 0.4 : 1,
      boxShadow: selected ? '0 0 0 3px rgba(55,138,221,0.15)' : 'none',
      transition: 'all 0.12s ease',
      flexShrink: 0,
    }}>
      <svg width={inner} height={inner} viewBox="0 0 28 28" fill="none">
        <circle cx="14" cy="14" r="11" fill="#EBF4FD" stroke="#378ADD" strokeWidth="1.5"/>
        <circle cx="14" cy="14" r="7" fill="none" stroke="#378ADD" strokeWidth="0.8" strokeDasharray="2 1.5" opacity="0.5"/>
        <line x1="14" y1="3" x2="14" y2="25" stroke="#378ADD" strokeWidth="0.8" opacity="0.3"/>
        <line x1="3" y1="14" x2="25" y2="14" stroke="#378ADD" strokeWidth="0.8" opacity="0.3"/>
        <path d="M11 14L13 12L15 14L13 16Z" fill="#378ADD"/>
        <path d="M14 11L16 13L14 15L12 13Z" fill="#185FA5" opacity="0.7"/>
        <circle cx="14" cy="14" r="2" fill="white"/>
      </svg>
    </div>
  )
}
