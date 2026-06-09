// ─── Icon exports ────────────────────────────────────────────────────────────
export { RouterIcon } from './RouterIcon'
export {
  // Layer 3 — Routing
  L3SwitchIcon,
  CoreSwitchIcon,
  DistSwitchIcon,
  AccessSwitchIcon,
  // Security
  FirewallIcon,
  IdsIcon,
  ZoneFirewallIcon,
  SiemIcon,
  VpnGatewayIcon,
  // Endpoints & Services
  ServerIcon,
  HostIcon,
  ApIcon,
  DnsIcon,
  DhcpIcon,
  // WAN & Cloud
  CloudIcon,
  NatIcon,
  IspIcon,
  // Special
  LoadBalancerIcon,
  ConsoleIcon,
  MonitorIcon,
} from './AllIcons'

export type { IconProps } from './AllIcons'

// ─── Node role type (mirrors netgen-core NodeRole enum) ──────────────────────
export type NodeRole =
  // Routing
  | 'Router'
  | 'L3Switch'
  | 'CoreSwitch'
  | 'DistSwitch'
  | 'AccessSwitch'
  // Security
  | 'Firewall'
  | 'Ids'
  | 'ZoneFirewall'
  | 'Siem'
  | 'VpnGateway'
  // Endpoints
  | 'Server'
  | 'Host'
  | 'Ap'
  | 'Dns'
  | 'Dhcp'
  // WAN & Cloud
  | 'Cloud'
  | 'Nat'
  | 'Isp'
  // Special
  | 'LoadBalancer'
  | 'Console'
  | 'Monitor'

// ─── Zone role type (mirrors netgen-core ZoneRole enum) ─────────────────────
export type ZoneRole =
  | 'Lan'
  | 'Wan'
  | 'Dmz'
  | 'Mgmt'
  | 'Transit'
  | 'Oob'
  | 'Guest'

// ─── Icon registry — maps NodeRole to component ──────────────────────────────
import type { ComponentType } from 'react'
import type { IconProps as IP } from './AllIcons'
import { RouterIcon } from './RouterIcon'
import {
  L3SwitchIcon, CoreSwitchIcon, DistSwitchIcon, AccessSwitchIcon,
  FirewallIcon, IdsIcon, ZoneFirewallIcon, SiemIcon, VpnGatewayIcon,
  ServerIcon, HostIcon, ApIcon, DnsIcon, DhcpIcon,
  CloudIcon, NatIcon, IspIcon,
  LoadBalancerIcon, ConsoleIcon, MonitorIcon,
} from './AllIcons'

export const NODE_ICON_REGISTRY: Record<NodeRole, ComponentType<IP>> = {
  // Routing
  Router:       RouterIcon,
  L3Switch:     L3SwitchIcon,
  CoreSwitch:   CoreSwitchIcon,
  DistSwitch:   DistSwitchIcon,
  AccessSwitch: AccessSwitchIcon,
  // Security
  Firewall:     FirewallIcon,
  Ids:          IdsIcon,
  ZoneFirewall: ZoneFirewallIcon,
  Siem:         SiemIcon,
  VpnGateway:   VpnGatewayIcon,
  // Endpoints
  Server:       ServerIcon,
  Host:         HostIcon,
  Ap:           ApIcon,
  Dns:          DnsIcon,
  Dhcp:         DhcpIcon,
  // WAN & Cloud
  Cloud:        CloudIcon,
  Nat:          NatIcon,
  Isp:          IspIcon,
  // Special
  LoadBalancer: LoadBalancerIcon,
  Console:      ConsoleIcon,
  Monitor:      MonitorIcon,
}

// ─── Zone color registry — maps ZoneRole to colors ──────────────────────────
interface ZoneColors {
  border: string
  fill:   string
  label:  { bg: string; text: string }
}

export const ZONE_COLOR_REGISTRY: Record<ZoneRole, ZoneColors> = {
  Lan:     { border: '#1D9E75', fill: 'rgba(29,158,117,0.03)',   label: { bg: '#edf7f2', text: '#0f6e56' } },
  Wan:     { border: '#888780', fill: 'rgba(136,135,128,0.03)',  label: { bg: '#f1efe8', text: '#5f5e5a' } },
  Dmz:     { border: '#D85A30', fill: 'rgba(216,90,48,0.04)',    label: { bg: '#fff0ec', text: '#993c1d' } },
  Mgmt:    { border: '#534AB7', fill: 'rgba(83,74,183,0.03)',    label: { bg: '#edf0ff', text: '#3b38a0' } },
  Transit: { border: '#378ADD', fill: 'rgba(55,138,221,0.03)',   label: { bg: '#edf4fd', text: '#185fa5' } },
  Oob:     { border: '#534AB7', fill: 'rgba(83,74,183,0.02)',    label: { bg: '#edf0ff', text: '#3b38a0' } },
  Guest:   { border: '#BA7517', fill: 'rgba(186,117,23,0.03)',   label: { bg: '#faeeda', text: '#854f0b' } },
}
