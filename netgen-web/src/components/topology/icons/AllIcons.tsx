// ─── Shared prop type ───────────────────────────────────────────────────────

export interface IconProps {
  size?: number
  selected?: boolean
  dimmed?: boolean
}

// ─── Helper ─────────────────────────────────────────────────────────────────

function iconWrap(
  size: number,
  selected: boolean,
  dimmed: boolean,
  bg: string,
  border: string,
  selectedBorder: string,
  children: React.ReactNode,
  radius = 8,
) {
  return (
    <div style={{
      width: size, height: size,
      background: bg,
      border: `${selected ? 2 : 1.5}px solid ${selected ? selectedBorder : border}`,
      borderRadius: radius,
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      opacity: dimmed ? 0.4 : 1,
      boxShadow: selected ? `0 0 0 3px ${selectedBorder}26` : 'none',
      transition: 'all 0.12s ease',
      flexShrink: 0,
    }}>
      {children}
    </div>
  )
}

// ═══════════════════════════════════════════════════════════════════════════
// LAYER 3 — ROUTING  (blue family)
// ═══════════════════════════════════════════════════════════════════════════

export function RouterIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#EBF4FD', '#378ADD', '#185FA5',
    <svg width={s} height={s} viewBox="0 0 28 28" fill="none">
      <circle cx="14" cy="14" r="11" fill="#EBF4FD" stroke="#378ADD" strokeWidth="1.5"/>
      <circle cx="14" cy="14" r="7" fill="none" stroke="#378ADD" strokeWidth="0.8" strokeDasharray="2 1.5" opacity="0.5"/>
      <line x1="14" y1="3" x2="14" y2="25" stroke="#378ADD" strokeWidth="0.8" opacity="0.3"/>
      <line x1="3" y1="14" x2="25" y2="14" stroke="#378ADD" strokeWidth="0.8" opacity="0.3"/>
      <path d="M11 14L13 12L15 14L13 16Z" fill="#378ADD"/>
      <path d="M14 11L16 13L14 15L12 13Z" fill="#185FA5" opacity="0.7"/>
      <circle cx="14" cy="14" r="2" fill="white"/>
    </svg>
  )
}

export function L3SwitchIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#EBF4FD', '#378ADD', '#185FA5',
    <svg width={s} height={s} viewBox="0 0 28 28" fill="none">
      <rect x="2" y="9" width="24" height="12" rx="2.5" fill="#EBF4FD" stroke="#378ADD" strokeWidth="1.5"/>
      <rect x="2" y="12" width="24" height="4" fill="#B5D4F4" opacity="0.5"/>
      <circle cx="8" cy="15" r="1.5" fill="#378ADD"/>
      <circle cx="14" cy="15" r="1.5" fill="#378ADD"/>
      <circle cx="20" cy="15" r="1.5" fill="#378ADD"/>
      <line x1="8" y1="21" x2="8" y2="25" stroke="#378ADD" strokeWidth="1.5"/>
      <line x1="14" y1="21" x2="14" y2="25" stroke="#378ADD" strokeWidth="1.5"/>
      <line x1="20" y1="21" x2="20" y2="25" stroke="#378ADD" strokeWidth="1.5"/>
      <path d="M14 4L17 8L14 12L11 8Z" fill="#378ADD"/>
    </svg>
  )
}

export function CoreSwitchIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.72)
  return iconWrap(size, selected, dimmed, '#EBF4FD', '#378ADD', '#185FA5',
    <svg width={s} height={s} viewBox="0 0 30 30" fill="none">
      <rect x="2" y="5" width="26" height="20" rx="3" fill="#EBF4FD" stroke="#378ADD" strokeWidth="1.5"/>
      <rect x="2" y="9" width="26" height="5" fill="#B5D4F4" opacity="0.6"/>
      <rect x="2" y="17" width="26" height="5" fill="#B5D4F4" opacity="0.35"/>
      <circle cx="25" cy="7" r="1.5" fill="#378ADD"/>
      <circle cx="25" cy="14" r="1.5" fill="#378ADD"/>
      <circle cx="25" cy="20" r="1.5" fill="#185FA5"/>
      <line x1="8" y1="25" x2="8" y2="28" stroke="#378ADD" strokeWidth="1.5"/>
      <line x1="14" y1="25" x2="14" y2="28" stroke="#378ADD" strokeWidth="1.5"/>
      <line x1="20" y1="25" x2="20" y2="28" stroke="#378ADD" strokeWidth="1.5"/>
    </svg>
  )
}

export function DistSwitchIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#EBF4FD', '#378ADD', '#185FA5',
    <svg width={s} height={s} viewBox="0 0 28 28" fill="none">
      <rect x="2" y="8" width="24" height="6" rx="2" fill="#EBF4FD" stroke="#378ADD" strokeWidth="1.5"/>
      <rect x="2" y="16" width="24" height="6" rx="2" fill="#EBF4FD" stroke="#378ADD" strokeWidth="1.2"/>
      <circle cx="22" cy="11" r="1.5" fill="#378ADD"/>
      <circle cx="22" cy="19" r="1.5" fill="#378ADD"/>
      <line x1="8" y1="22" x2="8" y2="26" stroke="#378ADD" strokeWidth="1.5"/>
      <line x1="14" y1="22" x2="14" y2="26" stroke="#378ADD" strokeWidth="1.5"/>
      <line x1="20" y1="22" x2="20" y2="26" stroke="#378ADD" strokeWidth="1.5"/>
      <line x1="8" y1="2" x2="8" y2="8" stroke="#378ADD" strokeWidth="1.5"/>
      <line x1="14" y1="2" x2="14" y2="8" stroke="#378ADD" strokeWidth="1.5"/>
      <line x1="20" y1="2" x2="20" y2="8" stroke="#378ADD" strokeWidth="1.5"/>
    </svg>
  )
}

export function AccessSwitchIcon({ size = 38, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#EBF4FD', '#85B7EB', '#378ADD',
    <svg width={s} height={s} viewBox="0 0 26 26" fill="none">
      <rect x="2" y="9" width="22" height="10" rx="2" fill="#EBF4FD" stroke="#378ADD" strokeWidth="1.2"/>
      <circle cx="8" cy="14" r="1.5" fill="#378ADD"/>
      <circle cx="13" cy="14" r="1.5" fill="#378ADD"/>
      <circle cx="18" cy="14" r="1.5" fill="#378ADD"/>
      <line x1="8" y1="19" x2="8" y2="23" stroke="#378ADD" strokeWidth="1.2"/>
      <line x1="13" y1="19" x2="13" y2="23" stroke="#378ADD" strokeWidth="1.2"/>
      <line x1="18" y1="19" x2="18" y2="23" stroke="#378ADD" strokeWidth="1.2"/>
      <line x1="8" y1="3" x2="8" y2="9" stroke="#378ADD" strokeWidth="1.2"/>
      <line x1="13" y1="3" x2="13" y2="9" stroke="#378ADD" strokeWidth="1.2"/>
      <line x1="18" y1="3" x2="18" y2="9" stroke="#378ADD" strokeWidth="1.2"/>
    </svg>
  )
}

// ═══════════════════════════════════════════════════════════════════════════
// SECURITY  (orange/coral family)
// ═══════════════════════════════════════════════════════════════════════════

export function FirewallIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#FFF0EC', '#D85A30', '#993C1D',
    <svg width={s} height={s} viewBox="0 0 28 28" fill="none">
      <path d="M14 3L23 7L23 15C23 20.5 18.5 25 14 27C9.5 25 5 20.5 5 15L5 7Z"
        fill="#FFF0EC" stroke="#D85A30" strokeWidth="1.5" strokeLinejoin="round"/>
      <rect x="9" y="11" width="10" height="8" rx="1.5" fill="none" stroke="#D85A30" strokeWidth="1.2"/>
      <line x1="11.5" y1="11" x2="11.5" y2="19" stroke="#D85A30" strokeWidth="0.8" opacity="0.5"/>
      <line x1="14" y1="11" x2="14" y2="19" stroke="#D85A30" strokeWidth="0.8" opacity="0.5"/>
      <line x1="16.5" y1="11" x2="16.5" y2="19" stroke="#D85A30" strokeWidth="0.8" opacity="0.5"/>
      <line x1="9" y1="13.5" x2="19" y2="13.5" stroke="#D85A30" strokeWidth="0.8" opacity="0.5"/>
      <line x1="9" y1="16" x2="19" y2="16" stroke="#D85A30" strokeWidth="0.8" opacity="0.5"/>
    </svg>
  )
}

export function IdsIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#FFF0EC', '#D85A30', '#993C1D',
    <svg width={s} height={s} viewBox="0 0 28 28" fill="none">
      <path d="M14 3L23 7L23 15C23 20.5 18.5 25 14 27C9.5 25 5 20.5 5 15L5 7Z"
        fill="#FFF0EC" stroke="#D85A30" strokeWidth="1.5" strokeLinejoin="round"/>
      <circle cx="14" cy="16" r="6" fill="none" stroke="#D85A30" strokeWidth="1.3"/>
      <circle cx="14" cy="16" r="2.5" fill="#D85A30"/>
      <line x1="14" y1="8" x2="14" y2="10" stroke="#D85A30" strokeWidth="1.3" strokeLinecap="round"/>
      <line x1="14" y1="22" x2="14" y2="24" stroke="#D85A30" strokeWidth="1.3" strokeLinecap="round"/>
      <line x1="7" y1="16" x2="9" y2="16" stroke="#D85A30" strokeWidth="1.3" strokeLinecap="round"/>
      <line x1="19" y1="16" x2="21" y2="16" stroke="#D85A30" strokeWidth="1.3" strokeLinecap="round"/>
    </svg>
  )
}

export function ZoneFirewallIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.72)
  return iconWrap(size, selected, dimmed, '#FFF0EC', '#D85A30', '#993C1D',
    <svg width={s} height={s} viewBox="0 0 30 30" fill="none">
      <rect x="2" y="4" width="26" height="22" rx="3" fill="#FFF0EC" stroke="#D85A30" strokeWidth="1.5"/>
      <rect x="2" y="4" width="26" height="8" rx="3" fill="#F5C4B3" opacity="0.5"/>
      <rect x="2" y="8" width="26" height="4" fill="#F5C4B3" opacity="0.3"/>
      <text x="15" y="10" textAnchor="middle" fontSize="5.5" fontFamily="monospace"
        fill="#993C1D" fontWeight="700">ACL</text>
      <line x1="6" y1="16" x2="13" y2="16" stroke="#D85A30" strokeWidth="1.2"/>
      <line x1="15" y1="16" x2="24" y2="16" stroke="#D85A30" strokeWidth="1.2" strokeDasharray="2 2"/>
      <line x1="6" y1="20" x2="24" y2="20" stroke="#D85A30" strokeWidth="1" strokeDasharray="2 2" opacity="0.5"/>
      <line x1="6" y1="24" x2="20" y2="24" stroke="#D85A30" strokeWidth="1" strokeDasharray="2 2" opacity="0.3"/>
    </svg>
  )
}

export function SiemIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#FFF0EC', '#D85A30', '#993C1D',
    <svg width={s} height={s} viewBox="0 0 28 28" fill="none">
      <ellipse cx="14" cy="20" rx="10" ry="4" fill="#FFF0EC" stroke="#D85A30" strokeWidth="1.2"/>
      <ellipse cx="14" cy="14" rx="10" ry="4" fill="#FFF0EC" stroke="#D85A30" strokeWidth="1.2"/>
      <ellipse cx="14" cy="8" rx="10" ry="4" fill="#FFF0EC" stroke="#D85A30" strokeWidth="1.5"/>
      <line x1="4" y1="8" x2="4" y2="20" stroke="#D85A30" strokeWidth="1.2"/>
      <line x1="24" y1="8" x2="24" y2="20" stroke="#D85A30" strokeWidth="1.2"/>
      <text x="14" y="10.5" textAnchor="middle" fontSize="4.5" fontFamily="monospace"
        fill="#993C1D" fontWeight="700">LOG</text>
    </svg>
  )
}

export function VpnGatewayIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#FFF0EC', '#D85A30', '#993C1D',
    <svg width={s} height={s} viewBox="0 0 28 28" fill="none">
      <rect x="5" y="5" width="18" height="16" rx="2.5" fill="#FFF0EC" stroke="#D85A30" strokeWidth="1.5"/>
      <rect x="5" y="5" width="18" height="6" rx="2.5" fill="#F5C4B3" opacity="0.5"/>
      <rect x="5" y="8" width="18" height="3" fill="#F5C4B3" opacity="0.3"/>
      <text x="14" y="10" textAnchor="middle" fontSize="4.5" fontFamily="monospace"
        fill="#993C1D" fontWeight="700">VPN</text>
      <line x1="8" y1="15" x2="20" y2="15" stroke="#D85A30" strokeWidth="1.2"/>
      <line x1="8" y1="18" x2="16" y2="18" stroke="#D85A30" strokeWidth="1" opacity="0.6"/>
      <line x1="14" y1="21" x2="14" y2="25" stroke="#D85A30" strokeWidth="1.5"/>
      <line x1="10" y1="25" x2="18" y2="25" stroke="#D85A30" strokeWidth="2"/>
    </svg>
  )
}

// ═══════════════════════════════════════════════════════════════════════════
// ENDPOINTS & SERVICES  (green family)
// ═══════════════════════════════════════════════════════════════════════════

export function ServerIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#E1F5EE', '#1D9E75', '#0F6E56',
    <svg width={s} height={s} viewBox="0 0 28 28" fill="none">
      <rect x="5" y="3" width="18" height="22" rx="2.5" fill="#E1F5EE" stroke="#1D9E75" strokeWidth="1.5"/>
      <rect x="5" y="7" width="18" height="5" fill="#9FE1CB" opacity="0.5"/>
      <rect x="5" y="14" width="18" height="5" fill="#9FE1CB" opacity="0.35"/>
      <circle cx="20" cy="9.5" r="1.5" fill="#1D9E75"/>
      <circle cx="20" cy="16.5" r="1.5" fill="#1D9E75"/>
      <circle cx="20" cy="21.5" r="1.2" fill="#9FE1CB"/>
      <line x1="10" y1="25" x2="10" y2="28" stroke="#1D9E75" strokeWidth="1.5"/>
      <line x1="18" y1="25" x2="18" y2="28" stroke="#1D9E75" strokeWidth="1.5"/>
      <line x1="7" y1="28" x2="21" y2="28" stroke="#1D9E75" strokeWidth="2"/>
    </svg>
  )
}

export function HostIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.72)
  return iconWrap(size, selected, dimmed, '#E1F5EE', '#1D9E75', '#0F6E56',
    <svg width={s} height={s} viewBox="0 0 30 30" fill="none">
      <rect x="2" y="4" width="26" height="18" rx="3" fill="#E1F5EE" stroke="#1D9E75" strokeWidth="1.5"/>
      <rect x="5" y="7" width="20" height="11" rx="1.5" fill="#9FE1CB" opacity="0.3"/>
      <rect x="2" y="22" width="26" height="4" rx="2" fill="#E1F5EE" stroke="#1D9E75" strokeWidth="1"/>
      <line x1="15" y1="26" x2="15" y2="29" stroke="#1D9E75" strokeWidth="1.5"/>
      <line x1="10" y1="29" x2="20" y2="29" stroke="#1D9E75" strokeWidth="2"/>
    </svg>
  )
}

export function ApIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#E1F5EE', '#1D9E75', '#0F6E56',
    <svg width={s} height={s} viewBox="0 0 28 28" fill="none">
      <path d="M6 14C6 9.58 9.58 6 14 6C18.42 6 22 9.58 22 14"
        fill="none" stroke="#1D9E75" strokeWidth="1.8" strokeLinecap="round"/>
      <path d="M9 14C9 11.24 11.24 9 14 9C16.76 9 19 11.24 19 14"
        fill="none" stroke="#1D9E75" strokeWidth="1.5" strokeLinecap="round"/>
      <path d="M11.5 14C11.5 12.62 12.62 11.5 14 11.5C15.38 11.5 16.5 12.62 16.5 14"
        fill="none" stroke="#1D9E75" strokeWidth="1.2" strokeLinecap="round"/>
      <circle cx="14" cy="15.5" r="2" fill="#1D9E75"/>
      <line x1="14" y1="17.5" x2="14" y2="23" stroke="#1D9E75" strokeWidth="1.5"/>
      <rect x="9" y="23" width="10" height="3" rx="1.5" fill="#E1F5EE" stroke="#1D9E75" strokeWidth="1"/>
    </svg>
  )
}

export function DnsIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#E1F5EE', '#1D9E75', '#0F6E56',
    <svg width={s} height={s} viewBox="0 0 28 28" fill="none">
      <ellipse cx="14" cy="18" rx="9" ry="5" fill="#E1F5EE" stroke="#1D9E75" strokeWidth="1.2"/>
      <ellipse cx="14" cy="11" rx="9" ry="5" fill="#E1F5EE" stroke="#1D9E75" strokeWidth="1.5"/>
      <line x1="5" y1="11" x2="5" y2="18" stroke="#1D9E75" strokeWidth="1.2"/>
      <line x1="23" y1="11" x2="23" y2="18" stroke="#1D9E75" strokeWidth="1.2"/>
      <text x="14" y="13.5" textAnchor="middle" fontSize="4.5" fontFamily="monospace"
        fill="#0F6E56" fontWeight="700">DNS</text>
    </svg>
  )
}

export function DhcpIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#E1F5EE', '#1D9E75', '#0F6E56',
    <svg width={s} height={s} viewBox="0 0 28 28" fill="none">
      <rect x="7" y="3" width="14" height="20" rx="2.5" fill="#E1F5EE" stroke="#1D9E75" strokeWidth="1.5"/>
      <rect x="7" y="7" width="14" height="4" fill="#9FE1CB" opacity="0.5"/>
      <rect x="7" y="13" width="14" height="4" fill="#9FE1CB" opacity="0.35"/>
      <circle cx="18" cy="9" r="1.2" fill="#1D9E75"/>
      <circle cx="18" cy="15" r="1.2" fill="#1D9E75"/>
      <rect x="9" y="19" width="10" height="3" rx="1.2" fill="#9FE1CB" opacity="0.5"/>
      <text x="14" y="21.5" textAnchor="middle" fontSize="4" fontFamily="monospace"
        fill="#0F6E56" fontWeight="700">DHCP</text>
    </svg>
  )
}

// ═══════════════════════════════════════════════════════════════════════════
// WAN & CLOUD  (gray / purple family)
// ═══════════════════════════════════════════════════════════════════════════

export function CloudIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.75)
  return iconWrap(size, selected, dimmed, '#F1EFE8', '#C8C6BC', '#888780',
    <svg width={s} height={s} viewBox="0 0 32 24" fill="none">
      <path d="M7 18Q7 12 12 12Q12 5 18 5Q23 5 24 10Q29 10 29 15Q31 15 31 18Q31 22 26 22L9 22Q4 22 4 18Q4 14 7 18Z"
        fill="#F1EFE8" stroke="#888780" strokeWidth="1.3" strokeLinejoin="round"/>
      <circle cx="13" cy="16" r="1.5" fill="#888780"/>
      <circle cx="18" cy="14.5" r="1.5" fill="#888780"/>
      <circle cx="23" cy="16" r="1.5" fill="#888780"/>
    </svg>
  )
}

export function NatIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.72)
  return iconWrap(size, selected, dimmed, '#EEEDFE', '#7F77DD', '#534AB7',
    <svg width={s} height={s} viewBox="0 0 30 30" fill="none">
      <rect x="2" y="7" width="26" height="16" rx="3" fill="#EEEDFE" stroke="#534AB7" strokeWidth="1.5"/>
      <line x1="2" y1="15" x2="28" y2="15" stroke="#534AB7" strokeWidth="1" strokeDasharray="3 2" opacity="0.5"/>
      <text x="15" y="12.5" textAnchor="middle" fontSize="5" fontFamily="monospace"
        fill="#534AB7" fontWeight="700">NAT</text>
      <text x="8" y="21" textAnchor="middle" fontSize="4.5" fontFamily="monospace"
        fill="#534AB7">10.x</text>
      <text x="22" y="21" textAnchor="middle" fontSize="4.5" fontFamily="monospace"
        fill="#534AB7">pub</text>
      <line x1="0" y1="15" x2="2" y2="15" stroke="#534AB7" strokeWidth="1.5"/>
      <line x1="28" y1="15" x2="30" y2="15" stroke="#534AB7" strokeWidth="1.5"/>
    </svg>
  )
}

export function IspIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.72)
  return iconWrap(size, selected, dimmed, '#EEEDFE', '#7F77DD', '#534AB7',
    <svg width={s} height={s} viewBox="0 0 30 30" fill="none">
      <rect x="2" y="9" width="26" height="13" rx="3" fill="#EEEDFE" stroke="#534AB7" strokeWidth="1.5"/>
      <circle cx="9" cy="15.5" r="3.5" fill="#534AB7"/>
      <circle cx="15" cy="15.5" r="3.5" fill="#7F77DD"/>
      <circle cx="21" cy="15.5" r="3.5" fill="#AFA9EC"/>
      <line x1="15" y1="22" x2="15" y2="27" stroke="#534AB7" strokeWidth="1.5"/>
    </svg>
  )
}

// ═══════════════════════════════════════════════════════════════════════════
// SPECIAL
// ═══════════════════════════════════════════════════════════════════════════

export function LoadBalancerIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.72)
  return iconWrap(size, selected, dimmed, '#FAEEDA', '#BA7517', '#854F0B',
    <svg width={s} height={s} viewBox="0 0 30 30" fill="none">
      <rect x="2" y="4" width="26" height="22" rx="2.5" fill="#FAEEDA" stroke="#BA7517" strokeWidth="1.5"/>
      <rect x="2" y="4" width="26" height="7" rx="2.5" fill="#FAC775" opacity="0.5"/>
      <rect x="2" y="8" width="26" height="3" fill="#FAC775" opacity="0.3"/>
      <circle cx="26" cy="7.5" r="1.8" fill="#BA7517"/>
      <line x1="6" y1="16" x2="24" y2="16" stroke="#BA7517" strokeWidth="1.2"/>
      <line x1="6" y1="20" x2="20" y2="20" stroke="#BA7517" strokeWidth="1" opacity="0.7"/>
      <line x1="6" y1="23" x2="16" y2="23" stroke="#BA7517" strokeWidth="1" opacity="0.5"/>
    </svg>
  )
}

export function ConsoleIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.65)
  return iconWrap(size, selected, dimmed, '#FAEEDA', '#BA7517', '#854F0B',
    <svg width={s} height={s} viewBox="0 0 28 28" fill="none">
      <rect x="7" y="3" width="14" height="22" rx="3" fill="#FAEEDA" stroke="#BA7517" strokeWidth="1.5"/>
      <rect x="7" y="7" width="14" height="5" fill="#FAC775" opacity="0.5"/>
      <rect x="10" y="15" width="8" height="2" rx="1" fill="#BA7517" opacity="0.5"/>
      <rect x="10" y="19" width="6" height="2" rx="1" fill="#BA7517" opacity="0.35"/>
      <circle cx="14" cy="5" r="1.5" fill="#BA7517"/>
    </svg>
  )
}

export function MonitorIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  const s = Math.round(size * 0.72)
  return iconWrap(size, selected, dimmed, '#FCEBEB', '#E24B4A', '#A32D2D',
    <svg width={s} height={s} viewBox="0 0 30 30" fill="none">
      <rect x="2" y="4" width="26" height="20" rx="2.5" fill="#FCEBEB" stroke="#A32D2D" strokeWidth="1.5"/>
      <rect x="2" y="4" width="26" height="7" rx="2.5" fill="#F7C1C1" opacity="0.4"/>
      <rect x="2" y="8" width="26" height="3" fill="#F7C1C1" opacity="0.25"/>
      <text x="15" y="10" textAnchor="middle" fontSize="4.5" fontFamily="monospace"
        fill="#A32D2D" fontWeight="700">MONITOR</text>
      <rect x="5" y="15" width="6" height="2.5" rx="1" fill="#E24B4A" opacity="0.6"/>
      <rect x="5" y="19" width="14" height="2.5" rx="1" fill="#E24B4A" opacity="0.4"/>
      <circle cx="24" cy="16.5" r="2.5" fill="#E24B4A" opacity="0.7"/>
    </svg>
  )
}
