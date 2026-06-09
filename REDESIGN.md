# Labyrinth — UI Redesign Spec

> Clean minimal SaaS, light theme. Professional engineering tool aesthetic.  
> Reference: Linear / Vercel dashboard — but purpose-built for network engineers.

---

## Table of Contents

1. [Design Direction](#1-design-direction)
2. [Design Tokens](#2-design-tokens)
3. [Layout Architecture](#3-layout-architecture)
4. [Component Specs](#4-component-specs)
5. [Network Icon Library](#5-network-icon-library)
6. [Icon Registry — Engine Mapping](#6-icon-registry--engine-mapping)
7. [Topology Canvas](#7-topology-canvas)
8. [Implementation Guide for Claude Code](#8-implementation-guide-for-claude-code)
9. [File Structure](#9-file-structure)

---

## 1. Design Direction

### What it is
Clean, minimal, light. Every element earns its place. No gradients, no glassmorphism, no dark backgrounds. Feels like a professional tool that a network engineer would trust — not a consumer app.

### What it is NOT
- Not dark-by-default
- Not purple-gradient AI aesthetic
- Not generic SaaS with rounded cards everywhere
- Not over-animated

### Personality
Precise. Confident. The kind of UI that makes you feel like you're using a tool built by engineers for engineers.

### Inspiration reference
The mockup generated from the AI image prompt is the north star. Key elements to preserve:
- Left sidebar with icon-only navigation
- Topology as the dominant canvas element
- Dashed zone boundaries with subtle fills
- Bottom device detail panel (drawer style)
- Right AI chat panel with monospace config output
- Clean top bar with breadcrumb

---

## 2. Design Tokens

### Colors

```typescript
// tokens/colors.ts
export const colors = {
  // Backgrounds
  bg: {
    page:    '#f5f4f1',   // warm off-white page background
    surface: '#ffffff',   // panels, sidebar, topbar
    canvas:  '#fafaf8',   // topology canvas
    hover:   '#f0ede6',   // hover state
    input:   '#f5f4f1',   // input fields
  },

  // Borders
  border: {
    subtle:  '#e8e6e0',   // panel separators
    default: '#d8d6cc',   // cards, inputs
    strong:  '#c8c6bc',   // active borders
  },

  // Text
  text: {
    primary:   '#1a1a18',
    secondary: '#5f5e5a',
    tertiary:  '#888780',
    disabled:  '#b4b2a9',
    inverse:   '#ffffff',
  },

  // Accent — single blue
  accent: {
    50:  '#edf4fd',
    100: '#b5d4f4',
    400: '#378add',
    600: '#185fa5',
    800: '#0c447c',
  },

  // Semantic
  success: { bg: '#edf7f2', text: '#0f6e56', dot: '#1d9e75' },
  danger:  { bg: '#fff0ec', text: '#993c1d', dot: '#d85a30' },
  warning: { bg: '#faeeda', text: '#854f0b', dot: '#ef9f27' },
  info:    { bg: '#edf0ff', text: '#3b38a0', dot: '#4340b0' },

  // Device type colors (for node icons)
  device: {
    routing:  { bg: '#EBF4FD', border: '#378ADD', icon: '#185FA5' },
    security: { bg: '#FFF0EC', border: '#D85A30', icon: '#993C1D' },
    endpoint: { bg: '#E1F5EE', border: '#1D9E75', icon: '#0F6E56' },
    wan:      { bg: '#F1EFE8', border: '#888780', icon: '#5F5E5A' },
    special:  { bg: '#EEEDFE', border: '#534AB7', icon: '#3C3489' },
  },

  // Zone colors (for zone boundary boxes)
  zone: {
    edge:     { border: '#378ADD', fill: 'rgba(55,138,221,0.04)',  label: { bg: '#edf4fd', text: '#185fa5' } },
    core:     { border: '#378ADD', fill: 'rgba(55,138,221,0.03)',  label: { bg: '#edf4fd', text: '#185fa5' } },
    dmz:      { border: '#D85A30', fill: 'rgba(216,90,48,0.04)',   label: { bg: '#fff0ec', text: '#993c1d' } },
    lan:      { border: '#1D9E75', fill: 'rgba(29,158,117,0.03)',  label: { bg: '#edf7f2', text: '#0f6e56' } },
    mgmt:     { border: '#534AB7', fill: 'rgba(83,74,183,0.03)',   label: { bg: '#edf0ff', text: '#3b38a0' } },
    guest:    { border: '#BA7517', fill: 'rgba(186,117,23,0.03)',  label: { bg: '#faeeda', text: '#854f0b' } },
    clinical: { border: '#D85A30', fill: 'rgba(216,90,48,0.03)',   label: { bg: '#fff0ec', text: '#993c1d' } },
    wan:      { border: '#888780', fill: 'rgba(136,135,128,0.03)', label: { bg: '#f1efe8', text: '#5f5e5a' } },
  },
}
```

### Typography

```typescript
// tokens/typography.ts
export const typography = {
  fonts: {
    sans:  '"Inter", -apple-system, BlinkMacSystemFont, sans-serif',
    mono:  '"JetBrains Mono", "Fira Code", "Cascadia Code", monospace',
  },
  sizes: {
    xs:   '10px',
    sm:   '11px',
    base: '12px',
    md:   '13px',
    lg:   '14px',
    xl:   '16px',
  },
  weights: {
    normal:  400,
    medium:  500,
    semibold: 600,
  },
  lineHeights: {
    tight:  1.3,
    base:   1.5,
    loose:  1.7,
  },
}
```

### Spacing & Radius

```typescript
// tokens/spacing.ts
export const spacing = {
  1: '4px',
  2: '8px',
  3: '12px',
  4: '16px',
  5: '20px',
  6: '24px',
}

export const radius = {
  sm:  '5px',
  md:  '7px',
  lg:  '9px',
  xl:  '12px',
  full: '9999px',
}
```

---

## 3. Layout Architecture

### Overall structure

```
┌─────────────────────────────────────────────────────────────────────┐
│  TOPBAR (44px)                                                       │
├──────┬──────────────────────────────────────────────────┬───────────┤
│      │  OUTPUT TABS (36px)                              │           │
│  S   ├──────────────────────────────────────────────────┤   CHAT    │
│  I   │  CANVAS TOOLBAR (38px)                           │   PANEL   │
│  D   ├──────────────────────────────────────────────────┤  (300px)  │
│  E   │                                                  │           │
│  B   │  TOPOLOGY CANVAS (flex 1)                        │           │
│  A   │                                                  │           │
│  R   │                                                  │           │
│      ├──────────────────────────────────────────────────┤           │
│(48px)│  DEVICE PANEL (200px, collapsible)               │           │
└──────┴──────────────────────────────────────────────────┴───────────┘
```

### Height breakdown

```
Total viewport: 100vh

Topbar:            44px  fixed
Main area:         calc(100vh - 44px)
  Sidebar:         48px wide, full height
  Canvas area:     flex 1
    Output tabs:   36px
    Canvas toolbar: 38px
    Canvas:        flex 1
    Device panel:  200px (collapsible to 0)
  Chat panel:      300px wide, full height
```

---

## 4. Component Specs

### 4.1 Topbar

Height: 44px. Background: white. Bottom border: `border.subtle`.

**Left:**
- Logo mark (22×22 SVG hexagon with internal lines)
- Logo text "Labyrinth" — 14px, weight 600, letter-spacing -0.02em
- Breadcrumb separator
- Project name — 13px, weight 500
- Dropdown chevron (12×12)
- Saved pill: `{bg: '#edf7f2', color: '#0f6e56'}`, 5px green dot, "Saved" text

**Right:**
- "History" ghost button (clock icon + text)
- "Audit" ghost button (checkmark icon + text) — only shows when AI audit available
- "Export" primary button (dark background)
- User avatar (26×26 circle, initials)

### 4.2 Sidebar

Width: 48px. Background: white. Right border: `border.subtle`.

Icon nav items: 36×36px, border-radius 8px.

States:
- Default: `color: text.tertiary`, transparent bg
- Hover: `bg: bg.hover`, `color: text.primary`
- Active: `bg: '#edf0ff'`, `color: '#4340b0'`

Tooltips: appear on hover, positioned `left: 46px`, dark pill style.

Nav items (top to bottom):
```
Overview       grid-2x2 icon
Topology       node-graph icon (active by default)
Devices        table icon
IPAM           ip-grid icon
VLANs          stack icon
─ divider ─
Zones          hexagon icon
Routing        route icon
Policies       shield icon
─ divider ─
[bottom]
Settings       info-circle icon
```

### 4.3 Output Tabs

Height: 36px. Background: white. Bottom border.

Tab style: 
- Default: 11px, weight 500, `color: text.tertiary`
- Active: `color: text.primary`, 2px bottom border `#1a1a18`
- Count badge: `{bg: '#edf0ff', color: '#4340b0'}`, 9px, pill shape

Tabs:
```
Topology | Diagram | Configs [12] | IP Plan | Security | Simulation
```

### 4.4 Canvas Toolbar

Height: 38px. Background: white. Bottom border.

**Left tools:**
- Cursor tool (arrow icon) — active by default
- Rectangle tool (box icon)  
- Line tool
- ─ separator ─
- Connection tool (arrow-right icon)
- Pan tool (circle icon)

**Right controls:**
- Zoom out button
- Zoom percentage (100%)
- Zoom in button
- ─ separator ─
- Fit to screen button

Tool buttons: 28×28px, radius 6px. Active: `{bg: '#edf0ff', color: '#4340b0'}`.

### 4.5 Canvas

Background: `#fafaf8`. Dot grid: `radial-gradient(circle, #d8d6cc 1px, transparent 1px)`, 20px spacing, 50% opacity.

Contains:
- Zone boundary boxes (dashed, colored by zone role)
- Device nodes (SVG icons with labels below)
- Connection lines (SVG lines/paths)
- Build timeline panel (top-right, 158px wide card)

### 4.6 Build Timeline Panel

Position: absolute, top-right of canvas. 158px wide card.
Background: white. Border: `border.subtle`. Border-radius: 9px. Padding: 10px 12px.

Section title: 10px, uppercase, letter-spacing 0.05em, `text.tertiary`.

Timeline items: flex row, 7px gap.

Dot states:
- Done: 14px circle, `bg: '#1d9e75'`, white checkmark
- Active: 14px circle, `bg: '#4340b0'`, white arrow
- Pending: 14px circle, `bg: '#f0ede6'`, `color: '#b4b2a9'`, `border: 1px solid #e0ddd6`

Text states:
- Done: 10px, `color: '#1d9e75'`
- Active: 10px, weight 600, `color: '#4340b0'`
- Pending: 10px, `color: '#b4b2a9'`

### 4.7 Device Detail Panel

Height: 200px. Background: white. Top border. Collapsible (collapse icon top-right).

**Header (10px 14px padding):**
- Device icon (28×28, rounded, zone-color tinted)
- Device name (13px, weight 600)
- Status indicator (5px dot + "Online" text, success green)
- Close × button

**Meta row (grid, 6 columns):**
- Labels: 10px, uppercase, letter-spacing 0.05em, `text.tertiary`
- Values: 12px, weight 500, `text.primary`
- Fields: Role, Vendor, Zone, IP Address, VLAN, Uptime

**Tabs:**
- Same style as output tabs
- Tabs: Interfaces, Routing, Config, Policies, "Why this? ↗"
- "Why this?" opens the decision log entry — always last, always has ↗ indicator

**Content (table):**
- 11px text, `font-family: mono` for IPs and interface names
- Status: green dot + "Up" in success color
- Border-bottom between rows: `#f0ede6`
- Column headers: 10px, uppercase, `text.tertiary`

### 4.8 Chat Panel

Width: 300px. Background: white. Left border.

**Header:**
- AI orb: 24px circle, purple gradient, white network icon inside
- "Labyrinth AI" — 13px, weight 600
- "BETA" tag: 9px, uppercase, `{bg: '#f0effe', color: '#534ab7'}`
- Pin and expand buttons (right side)

**Messages:**
- AI messages: left-aligned, `bg: '#f5f4f1'`, radius `2px 10px 10px 10px`
- User messages: right-aligned, `bg: '#1a1a18'`, `color: white`, radius `10px 10px 2px 10px`
- Font: 12px, line-height 1.5
- Timestamps: 10px, `text.disabled`

**Code blocks in AI messages:**
- Background: `#1a1a18`
- Text: `#9fe1cb` (mint green monospace — matches terminal feel)
- Font: 10px monospace
- Radius: 6px
- Used for: path traces, config snippets, route tables

**Message actions (below AI messages):**
- Copy, thumbs up/down
- 10px, `text.disabled`, icon + text

**Input area:**
- Wrap: `bg: '#f5f4f1'`, border `#e0ddd6`, radius 10px, padding 8px 10px
- Placeholder: "Ask a question…"
- Send button: 26×26, `bg: '#1a1a18'`, white arrow icon, radius 7px
- Hint text: "↵ to send · shift+↵ new line", 10px, `text.disabled`

---

## 5. Network Icon Library

All icons are SVG-based, designed to look slightly real — recognizably professional without being photorealistic. Each icon uses the device type color system defined in the tokens.

### Icon size system

| Context | Size | Border radius |
|---|---|---|
| Topology canvas (large) | 46×46px | 8px |
| Topology canvas (default) | 42×42px | 8px |
| Topology canvas (small/access) | 38×38px | 7px |
| Device detail panel header | 28×28px | 6px |
| Sidebar nav | 16×16px | — |
| Table/list rows | 16×16px | — |

### Icon specs — Layer 3 Routing (blue family)

Colors: `bg: #EBF4FD`, `border: #378ADD`, `icon-detail: #185FA5`

**Router**
- Circle body with crossed arrows (Cisco-style)
- Outer circle + inner dashed circle (routing domain)
- Horizontal + vertical center lines (faint)
- Diamond-shaped directional arrows at center
- Visual metaphor: network routing in all directions

**L3 Switch**
- Wide rectangle body
- Horizontal port row with 4 circles
- Downward port legs
- Diamond routing arrow on top (indicates L3 capability)
- Mid-band fill (lighter blue) across port row

**Core Switch**
- Wider, taller rectangle (more imposing than access switch)
- Three horizontal rack-style bands
- Status indicator circles on right edge (active = dark blue)
- Port legs at bottom
- Visual weight communicates "backbone device"

**Distribution Switch**
- Two stacked rectangle bands
- Port legs top and bottom (uplinks + downlinks)
- Slightly less prominent than core

**Access Switch**
- Single narrow rectangle
- Three port circles
- Port legs downward to endpoints
- Port legs upward to distribution (dual uplinks)
- Simplest routing icon

### Icon specs — Security (orange/coral family)

Colors: `bg: #FFF0EC`, `border: #D85A30`, `icon-detail: #993C1D`

**Firewall**
- Shield body (path-based, not rectangle)
- Internal ACL grid (horizontal + vertical lines forming a mesh)
- Rounded rectangle outline inside shield
- Visual metaphor: filtering traffic

**IDS / IPS**
- Shield body
- Internal radar/eye motif (circle with crosshair)
- Center dot (detection point)
- Visual metaphor: watching traffic

**Zone Firewall**
- Rectangle body (not shield — zone-based, not perimeter)
- ACL label in header band
- Horizontal separating line between header and rules
- Arrow indicating traffic direction
- Dashed rule lines below

**Syslog / SIEM**
- Three stacked ellipses (database/log storage metaphor)
- "LOG" label on top ellipse
- Visual metaphor: storing and analyzing logs

**VPN Gateway**
- Rectangle body
- "VPN" label in tinted header band
- Horizontal rule lines (representing encrypted tunnels)
- Pole + base (standing gateway)

### Icon specs — Endpoints & Services (green family)

Colors: `bg: #E1F5EE`, `border: #1D9E75`, `icon-detail: #0F6E56`

**Server**
- Tall narrow rectangle
- Three rack-unit bands with status dots on right
- Port legs at bottom + ground line
- Rack mount base

**Host / PC**
- Wide flat rectangle (monitor)
- Inner screen area (lighter fill)
- Bottom bezel band
- Stand leg + base line

**Access Point**
- Three concentric arcs (expanding wifi symbol)
- Center dot
- Pole down to base rectangle
- Visual metaphor: wireless broadcast

**DNS Server**
- Two stacked ellipses (database shape)
- "DNS" label on top
- Connecting lines on sides

**DHCP Server**
- Tall narrow rectangle (server form factor)
- Three rack bands
- "DHCP" label in bottom tinted area
- Status dots

### Icon specs — WAN & Cloud (gray / purple family)

**Internet / Cloud**
Colors: `bg: #F1EFE8`, `border: #888780`
- Organic cloud path (3-bump silhouette)
- Three small dots inside (represents distributed internet)
- Warm gray tone (neutral, external)

**WAN Link**
Colors: gray, used on edges not nodes
- Two circles connected by dashed line with arrow
- Represents point-to-point or leased circuit

**NAT Gateway**
Colors: `bg: #EEEDFE`, `border: #534AB7` (purple — translation function)
- Rectangle body
- "NAT" label in header
- "10.x" / "pub" labels indicating private→public translation
- Arrows in and out (indicating address translation)

**Tunnel / VPN Link**
Colors: purple, used on edges
- Two circles with double dashed lines (parallel tunnels)
- Bidirectional arrows
- Visual metaphor: encrypted parallel path

**ISP / Carrier**
Colors: `bg: #EEEDFE`, `border: #534AB7`
- Wide rectangle
- Three circles of decreasing opacity (represents carrier infrastructure)
- Connecting line down (handoff point)

### Icon specs — Special

**Load Balancer**
Colors: amber (`bg: #FAEEDA`, `border: #BA7517`)
- Rectangle body with header band
- Status dot top-right
- Three horizontal rule lines of varying lengths (represents traffic distribution)

**OOB Console**
Colors: amber
- Narrow rectangle (console server form)
- Three bands

**NMS / Monitor**
Colors: red/danger (`bg: #FCEBEB`, `border: #A32D2D`)
- Rectangle with "MONITOR" header band
- Alert indicator (filled circle, danger red)
- Status lines

**Zone / VLAN**
Colors: green (`bg: #E1F5EE`, `border: #1D9E75`)
- Hexagon shape (zone = logical boundary)
- "VLAN" text center
- "zone" subtext above

**DMZ Zone**
Colors: blue (`bg: #E6F1FB`, `border: #185FA5`)
- Shield shape (security zone)
- "DMZ" text center
- Dashed line at bottom (demarcation)

---

## 6. Icon Registry — Engine Mapping

The frontend maintains a typed registry that maps `NodeRole` and `ZoneRole` enums (from `netgen-core`) to React components.

```typescript
// components/topology/IconRegistry.tsx

import {
  RouterIcon, L3SwitchIcon, CoreSwitchIcon, DistSwitchIcon, AccessSwitchIcon,
  FirewallIcon, IdsIcon, ZoneFirewallIcon, SiemIcon, VpnGatewayIcon,
  ServerIcon, HostIcon, ApIcon, DnsIcon, DhcpIcon,
  CloudIcon, NatIcon, IspIcon,
  LoadBalancerIcon, ConsoleIcon, MonitorIcon,
} from './icons'

// These enums mirror netgen-core's NodeRole and ZoneRole exactly
export type NodeRole =
  | 'Router' | 'L3Switch' | 'CoreSwitch' | 'DistSwitch' | 'AccessSwitch'
  | 'Firewall' | 'Ids' | 'ZoneFirewall' | 'Siem' | 'VpnGateway'
  | 'Server' | 'Host' | 'Ap' | 'Dns' | 'Dhcp'
  | 'Cloud' | 'Nat' | 'Isp'
  | 'LoadBalancer' | 'Console' | 'Monitor'

export type ZoneRole = 'Lan' | 'Wan' | 'Dmz' | 'Mgmt' | 'Transit' | 'Oob' | 'Guest'

export const NODE_ICON_REGISTRY: Record<NodeRole, React.ComponentType<IconProps>> = {
  Router:       RouterIcon,
  L3Switch:     L3SwitchIcon,
  CoreSwitch:   CoreSwitchIcon,
  DistSwitch:   DistSwitchIcon,
  AccessSwitch: AccessSwitchIcon,
  Firewall:     FirewallIcon,
  Ids:          IdsIcon,
  ZoneFirewall: ZoneFirewallIcon,
  Siem:         SiemIcon,
  VpnGateway:   VpnGatewayIcon,
  Server:       ServerIcon,
  Host:         HostIcon,
  Ap:           ApIcon,
  Dns:          DnsIcon,
  Dhcp:         DhcpIcon,
  Cloud:        CloudIcon,
  Nat:          NatIcon,
  Isp:          IspIcon,
  LoadBalancer: LoadBalancerIcon,
  Console:      ConsoleIcon,
  Monitor:      MonitorIcon,
}

// Each icon component accepts these props
interface IconProps {
  size?: number        // default 42
  selected?: boolean   // adds selection ring
  dimmed?: boolean     // reduces opacity (when another node is selected)
}
```

### Zone color registry

```typescript
// components/topology/ZoneRegistry.ts

export const ZONE_COLOR_REGISTRY: Record<ZoneRole, ZoneColors> = {
  Lan:     { border: '#1D9E75', fill: 'rgba(29,158,117,0.03)',   label: { bg: '#edf7f2', text: '#0f6e56' } },
  Wan:     { border: '#888780', fill: 'rgba(136,135,128,0.03)',  label: { bg: '#f1efe8', text: '#5f5e5a' } },
  Dmz:     { border: '#D85A30', fill: 'rgba(216,90,48,0.04)',    label: { bg: '#fff0ec', text: '#993c1d' } },
  Mgmt:    { border: '#534AB7', fill: 'rgba(83,74,183,0.03)',    label: { bg: '#edf0ff', text: '#3b38a0' } },
  Transit: { border: '#378ADD', fill: 'rgba(55,138,221,0.03)',   label: { bg: '#edf4fd', text: '#185fa5' } },
  Oob:     { border: '#534AB7', fill: 'rgba(83,74,183,0.02)',    label: { bg: '#edf0ff', text: '#3b38a0' } },
  Guest:   { border: '#BA7517', fill: 'rgba(186,117,23,0.03)',   label: { bg: '#faeeda', text: '#854f0b' } },
}
```

---

## 7. Topology Canvas

### Node anatomy

Each topology node is a React Flow custom node:

```
┌──────────────────┐
│  [Zone boundary] │
│                  │
│  ┌────────────┐  │
│  │ SVG Icon   │  │  ← 42×42px, border, bg from token
│  └────────────┘  │
│   Device Label   │  ← 9px, weight 500, text.secondary
│   IP Address     │  ← 8px, monospace, text.tertiary
│                  │
└──────────────────┘
```

Selected state: adds `box-shadow: 0 0 0 3px rgba(55,138,221,0.15)` + border color darkens to `accent.600`.

### Zone boundary boxes

Dashed rectangle, colored by zone role. Label sits top-left, slightly above the border:

```
┌ - - - - - - - - - - - - - - ┐
│ ZONE LABEL · subnet          │   ← 9px, 6px pill, zone label colors
│                               │
│  [nodes inside zone]          │
│                               │
└ - - - - - - - - - - - - - - ┘
```

Zone label pill: `{font-size: 9px, font-weight: 600, padding: 1px 6px, border-radius: 4px}`.

### Connection lines

```typescript
// Connection style by link type
const LINK_STYLES = {
  ethernet: { stroke: '#d8d6cc', strokeWidth: 1.5, strokeDasharray: 'none' },
  redundant: { stroke: '#e0ddd6', strokeWidth: 1, strokeDasharray: '3 2' },  // secondary/redundant links
  tunnel:   { stroke: '#AFA9EC', strokeWidth: 1.5, strokeDasharray: '4 3' }, // VPN/GRE tunnels
  wan:      { stroke: '#c8c6bc', strokeWidth: 2, strokeDasharray: 'none' },  // WAN circuits
}
```

No arrowheads on undirected links. Arrowheads only on directed links (e.g. default route, NAT direction).

### Canvas interaction behavior

| Interaction | Result |
|---|---|
| Click node | Opens device detail panel, highlights node with selection ring |
| Click zone | Highlights all devices in zone, sidebar shows zone policy |
| Click link | Shows link details tooltip (interface names, addresses, type) |
| Right-click node | Context menu: View Config, Rename, Change Vendor, Delete, Add Link |
| Right-click zone | Context menu: Add Device, Edit Zone Policy, View Security Rules |
| Drag node | Repositions node, auto-saves layout |
| Scroll | Zoom in/out |
| Drag canvas | Pan |
| Double-click canvas | Deselect all |

---

## 8. Implementation Guide for Claude Code

### Step 1 — Install dependencies

```bash
cd netgen-web
pnpm add reactflow @radix-ui/react-tooltip @radix-ui/react-context-menu
pnpm add class-variance-authority clsx tailwind-merge
```

### Step 2 — Set up design tokens

Create `src/tokens/` with `colors.ts`, `typography.ts`, `spacing.ts` as specified in Section 2. Export a unified `tokens` object.

Update `tailwind.config.ts` to use token values:

```typescript
// tailwind.config.ts
import { colors, typography, spacing, radius } from './src/tokens'

export default {
  theme: {
    extend: {
      colors: {
        bg: colors.bg,
        border: colors.border,
        text: colors.text,
        accent: colors.accent,
        device: colors.device,
        zone: colors.zone,
      },
      fontFamily: {
        sans: typography.fonts.sans.split(','),
        mono: typography.fonts.mono.split(','),
      },
      borderRadius: radius,
    }
  }
}
```

### Step 3 — Build the icon library

Create `src/components/topology/icons/` with one file per device type:

```
icons/
  index.ts              ← barrel export
  RouterIcon.tsx
  L3SwitchIcon.tsx
  CoreSwitchIcon.tsx
  DistSwitchIcon.tsx
  AccessSwitchIcon.tsx
  FirewallIcon.tsx
  IdsIcon.tsx
  ZoneFirewallIcon.tsx
  SiemIcon.tsx
  VpnGatewayIcon.tsx
  ServerIcon.tsx
  HostIcon.tsx
  ApIcon.tsx
  DnsIcon.tsx
  DhcpIcon.tsx
  CloudIcon.tsx
  NatIcon.tsx
  IspIcon.tsx
  LoadBalancerIcon.tsx
  ConsoleIcon.tsx
  MonitorIcon.tsx
```

Each icon component pattern:

```tsx
// icons/RouterIcon.tsx
interface IconProps {
  size?: number
  selected?: boolean
  dimmed?: boolean
}

export function RouterIcon({ size = 42, selected = false, dimmed = false }: IconProps) {
  return (
    <div
      style={{
        width: size,
        height: size,
        background: '#EBF4FD',
        border: `${selected ? 2 : 1.5}px solid ${selected ? '#185FA5' : '#378ADD'}`,
        borderRadius: 8,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        opacity: dimmed ? 0.4 : 1,
        boxShadow: selected ? '0 0 0 3px rgba(55,138,221,0.15)' : 'none',
        transition: 'all 0.12s ease',
      }}
    >
      <svg width={size * 0.65} height={size * 0.65} viewBox="0 0 28 28" fill="none">
        <circle cx="14" cy="14" r="11" fill="#EBF4FD" stroke="#378ADD" strokeWidth="1.5"/>
        <circle cx="14" cy="14" r="7" fill="none" stroke="#378ADD" strokeWidth="0.8"
          strokeDasharray="2 1.5" opacity="0.5"/>
        <line x1="14" y1="3" x2="14" y2="25" stroke="#378ADD" strokeWidth="0.8" opacity="0.3"/>
        <line x1="3" y1="14" x2="25" y2="14" stroke="#378ADD" strokeWidth="0.8" opacity="0.3"/>
        <path d="M11 14L13 12L15 14L13 16Z" fill="#378ADD"/>
        <path d="M14 11L16 13L14 15L12 13Z" fill="#185FA5" opacity="0.7"/>
        <circle cx="14" cy="14" r="2" fill="white"/>
      </svg>
    </div>
  )
}
```

Follow the same pattern for every icon, using the SVG paths described in Section 5.

### Step 4 — Build React Flow custom nodes

```tsx
// components/topology/nodes/DeviceNode.tsx
import { Handle, Position, NodeProps } from 'reactflow'
import { NODE_ICON_REGISTRY } from '../IconRegistry'

interface DeviceNodeData {
  label: string
  role: NodeRole
  ip?: string
  selected?: boolean
}

export function DeviceNode({ data, selected }: NodeProps<DeviceNodeData>) {
  const Icon = NODE_ICON_REGISTRY[data.role]

  return (
    <div className="flex flex-col items-center gap-1 cursor-pointer group">
      <Handle type="target" position={Position.Top} style={{ opacity: 0 }}/>
      <Icon size={42} selected={selected}/>
      <span style={{ fontSize: 9, fontWeight: 500, color: '#5f5e5a', textAlign: 'center' }}>
        {data.label}
      </span>
      {data.ip && (
        <span style={{ fontSize: 8, fontFamily: 'monospace', color: '#888780' }}>
          {data.ip}
        </span>
      )}
      <Handle type="source" position={Position.Bottom} style={{ opacity: 0 }}/>
    </div>
  )
}
```

### Step 5 — Zone overlay component

```tsx
// components/topology/ZoneOverlay.tsx
import { ZONE_COLOR_REGISTRY } from './ZoneRegistry'

interface ZoneOverlayProps {
  role: ZoneRole
  label: string
  subnet: string
  bounds: { x: number; y: number; width: number; height: number }
}

export function ZoneOverlay({ role, label, subnet, bounds }: ZoneOverlayProps) {
  const colors = ZONE_COLOR_REGISTRY[role]

  return (
    <div
      style={{
        position: 'absolute',
        left: bounds.x,
        top: bounds.y,
        width: bounds.width,
        height: bounds.height,
        border: `1.5px dashed ${colors.border}`,
        background: colors.fill,
        borderRadius: 10,
        pointerEvents: 'none',
      }}
    >
      <div
        style={{
          position: 'absolute',
          top: -10,
          left: 10,
          background: colors.label.bg,
          color: colors.label.text,
          fontSize: 9,
          fontWeight: 600,
          padding: '1px 6px',
          borderRadius: 4,
          letterSpacing: '0.03em',
          whiteSpace: 'nowrap',
        }}
      >
        {label} · {subnet}
      </div>
    </div>
  )
}
```

### Step 6 — Build the main layout

```tsx
// app/workspace/page.tsx
export default function WorkspacePage() {
  return (
    <div className="flex flex-col h-screen bg-[#f5f4f1]">
      <Topbar />
      <div className="flex flex-1 overflow-hidden">
        <Sidebar />
        <div className="flex flex-col flex-1 overflow-hidden">
          <OutputTabs />
          <CanvasToolbar />
          <div className="flex-1 relative overflow-hidden">
            <TopologyCanvas />
            <BuildTimeline />
          </div>
          <DevicePanel />
        </div>
        <ChatPanel />
      </div>
    </div>
  )
}
```

### Step 7 — Wire to API

The topology canvas receives data from the API in this shape (matching `netgen-core` output):

```typescript
interface TopologyData {
  nodes: Array<{
    id: string
    label: string
    role: NodeRole
    zone: string
    interfaces: Array<{
      name: string
      ip?: string
      description?: string
      status: 'up' | 'down'
    }>
    position: { x: number; y: number }
  }>
  edges: Array<{
    id: string
    source: string
    target: string
    sourceInterface: string
    targetInterface: string
    linkType: 'ethernet' | 'redundant' | 'tunnel' | 'wan'
  }>
  zones: Array<{
    id: string
    name: string
    role: ZoneRole
    subnet: string
    trustLevel: number
    bounds: { x: number; y: number; width: number; height: number }
  }>
}
```

Use `react-query` or SWR to fetch and subscribe to topology updates. The API streams graph version updates via WebSocket during generation — the canvas updates as each new version arrives.

### Step 8 — Streaming build

During generation, the backend streams `GraphVersion` events via WebSocket. Each event updates the canvas:

```typescript
// hooks/useTopologyStream.ts
export function useTopologyStream(projectId: string) {
  const [topology, setTopology] = useState<TopologyData | null>(null)
  const [buildLog, setBuildLog] = useState<BuildLogEntry[]>([])

  useEffect(() => {
    const ws = new WebSocket(`/api/projects/${projectId}/stream`)

    ws.onmessage = (event) => {
      const message = JSON.parse(event.data)

      if (message.type === 'graph_version') {
        setTopology(message.data)  // canvas updates immediately
      }

      if (message.type === 'rule_event') {
        setBuildLog(prev => [...prev, message.data])  // timeline updates
      }
    }

    return () => ws.close()
  }, [projectId])

  return { topology, buildLog }
}
```

---

## 9. File Structure

```
netgen-web/
  src/
    tokens/
      colors.ts
      typography.ts
      spacing.ts
      index.ts

    components/
      layout/
        Topbar.tsx
        Sidebar.tsx
        ChatPanel.tsx

      topology/
        TopologyCanvas.tsx
        CanvasToolbar.tsx
        OutputTabs.tsx
        BuildTimeline.tsx
        ZoneOverlay.tsx
        IconRegistry.tsx
        ZoneRegistry.ts

        nodes/
          DeviceNode.tsx
          CloudNode.tsx

        icons/
          index.ts
          RouterIcon.tsx
          L3SwitchIcon.tsx
          CoreSwitchIcon.tsx
          DistSwitchIcon.tsx
          AccessSwitchIcon.tsx
          FirewallIcon.tsx
          IdsIcon.tsx
          ZoneFirewallIcon.tsx
          SiemIcon.tsx
          VpnGatewayIcon.tsx
          ServerIcon.tsx
          HostIcon.tsx
          ApIcon.tsx
          DnsIcon.tsx
          DhcpIcon.tsx
          CloudIcon.tsx
          NatIcon.tsx
          IspIcon.tsx
          LoadBalancerIcon.tsx
          ConsoleIcon.tsx
          MonitorIcon.tsx

      panels/
        DevicePanel.tsx
        DevicePanelTabs.tsx
        WhyThisPanel.tsx

      chat/
        ChatPanel.tsx
        ChatMessage.tsx
        CodeBlock.tsx
        ChatInput.tsx

    hooks/
      useTopologyStream.ts
      useDeviceSelection.ts
      useZoneSelection.ts

    types/
      topology.ts       ← mirrors netgen-core types
      intent.ts
      audit.ts

    app/
      workspace/
        page.tsx
        layout.tsx
```

---

*Redesign version: 1.0 — June 2026*  
*Designed for: github.com/Mosope-ade/labyrinth*
