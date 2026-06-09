# Labyrinth — Test Guide

## What's been built

Phases 1–14 are complete. The full pipeline from natural language to verified network configs is implemented.

### Crate summary

| Crate | What it does |
|---|---|
| `netgen-core` | Pure Rust library. Schema → graph → rules → security → addressing → templates → verification → export. 254 tests. |
| `netgen-api` | Axum HTTP server. AI conversation layer, JWT auth, rate limiting, PostgreSQL (optional), WebSocket job stream. 86 tests. |
| `netgen-web` | Next.js 15 frontend. Chat UI, React Flow topology canvas, config viewer, IP plan, decision log, AI audit, exports. |
| `netgen-desktop` | Tauri v2 desktop shell. Runs the full pipeline in-process — no server needed. Local JSON project storage. |

### Pipeline stages (netgen-core)

```
NetworkIntent (serde-validated)
  → RuleEngine        (forward-chaining topology builder)
  → SecurityEngine    (hard gate: Block/AutoFix/Warn)
  → AddressingEngine  (deterministic IPv4/IPv6 allocation)
  → ConfigGenerator   (minijinja Cisco IOS templates)
  → LabellingEngine   (semantic zone names, {TYPE}-{ZONE}-{N} node names)
  → VerificationEngine (5 structural checks)
  → Exporters         (GNS3 zip, SVG diagram, CSV IP plan, Markdown docs)
```

---

## Prerequisites

```bash
# Rust (stable)
rustup update stable

# Node.js 20+
node --version

# Optional: PostgreSQL (API runs without it — DB routes return 503)
# Required for Tauri desktop build (already installed on this machine)
# sudo dnf install webkit2gtk4.1-devel javascriptcoregtk4.1-devel libsoup3-devel
```

---

## 1. Run the unit/integration test suite

```bash
# All tests — full workspace including desktop
cargo test --workspace

# Expected output:
#   netgen-api:   86 passed
#   netgen-core: 254 passed
#   (netgen-desktop has no unit tests — logic lives in netgen-core)
```

Run a specific crate:

```bash
cargo test -p netgen-core
cargo test -p netgen-api
```

Run a single test by name:

```bash
cargo test -p netgen-core zone_policy_completeness
cargo test -p netgen-api sliding_window_rate_limit
```

---

## 2. Run the API server

### 2a. Minimum setup (no database)

```bash
cp .env.example .env
# Edit .env — set AI_API_KEY to a real key
# DATABASE_URL can stay commented out — the server starts without it

cargo run -p netgen-api
# Listening on http://0.0.0.0:8080
```

The AI conversation endpoints (`/api/intent/chat`) work immediately. Project/auth endpoints return `503 Service Unavailable` until a database is connected.

### 2b. Full setup (with PostgreSQL)

```bash
# Create database
createdb labyrinth

# Run migrations
DATABASE_URL=postgres://localhost/labyrinth sqlx migrate run --source migrations/

# Add to .env
DATABASE_URL=postgres://localhost/labyrinth

cargo run -p netgen-api
```

### Smoke test the API

```bash
# Health (no auth needed)
curl http://localhost:8080/api/intent/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "I need a small hospital network with HIPAA compliance"}'

# Expected: JSON with session_id, phase, reply

# Register a user (requires DB)
curl -X POST http://localhost:8080/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{"email": "test@example.com", "password": "password123"}'

# Login
curl -X POST http://localhost:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "test@example.com", "password": "password123"}'
# Save the token from the response

# Create a project (requires DB + token)
curl -X POST http://localhost:8080/api/projects \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"name": "My Hospital Network"}'
```

---

## 3. Run the frontend

```bash
cd netgen-web
npm install
npm run dev
# Open http://localhost:3000
```

Point the frontend at your running API:

```bash
NEXT_PUBLIC_API_URL=http://localhost:8080 npm run dev
```

### What to test in the browser

**Auth flow**
- Open the app — auth modal appears
- Click "Continue as guest" to skip auth and test AI chat
- Register/login with a real account to test project persistence

**AI conversation (Gathering phase)**
- Type: `"3-site hospital network with ICU, clinical, and admin zones"`
- The phase badge should show **Gathering** (blue)
- Labyrinth asks follow-up questions; answer them
- When enough info is collected, it transitions to **Building** (amber) automatically

**Building phase**
- Watch the progress bar in the chat panel as pipeline stages run
- Tab bar switches to **Topology** automatically
- React Flow canvas renders zones (colored boxes) and devices within them

**Topology canvas**
- Nodes are grouped by security zone with color-coded borders
- Icons: ⬡ router · ◫ switch · ⬧ firewall · ☁ cloud · ⊛ AP
- Click a node to see interfaces and IP addresses in the detail panel
- Minimap in the bottom-right; zoom controls in the bottom-left
- Zone legend in the top-right

**Configs tab**
- Device list on the left; click a device to see its config
- IP addresses highlighted in green, section headers in blue
- Download individual `.cfg` or all configs via GNS3 export

**IP Plan tab**
- Allocations grouped by zone and VLAN
- Monospace table: node · interface · CIDR address

**Decisions tab (Why this?)**
- Full decision log with trigger badges: User / Rule / Security / AI
- Filter by trigger source
- Rule IDs shown where applicable

**AI Audit tab**
- Click "Run AI Audit" to send the design for a second-opinion review
- Structured output: critical findings, warnings, suggestions, compliance notes
- Toggle "Raw" to see the unformatted AI response

**Exports (header buttons — appear when project status is `ready`)**
- `GNS3` — downloads the GNS3 project archive
- `SVG` — downloads the architecture diagram
- `CSV` — downloads the IP plan spreadsheet

---

## 4. Build the frontend for production

```bash
cd netgen-web
npm run build
# Outputs to .next/ (server) or out/ (static)
```

---

## 5. Desktop app (Tauri v2)

> Requires WebKit GTK system libraries. On Fedora:
> ```bash
> sudo dnf install webkit2gtk4.1-devel javascriptcoregtk4.1-devel libsoup3-devel
> ```
> Then add `"netgen-desktop/src-tauri"` back to `members` in the root `Cargo.toml`.

```bash
# Set AI_API_KEY in your environment (no .env needed at runtime)
export AI_API_KEY=your-key-here

cd netgen-desktop
npm install
npm run dev   # starts Tauri dev window + Next.js dev server
```

The desktop app runs the full pipeline in-process — no HTTP server, no database. Projects are saved to `~/.local/share/labyrinth/projects/` as JSON files.

To build a distributable:

```bash
cd netgen-desktop
npm run build
# Output: netgen-desktop/src-tauri/target/release/bundle/
```

### Desktop-specific behaviour differences from the web app

| Feature | Web (netgen-api) | Desktop (Tauri) |
|---|---|---|
| AI conversation | HTTP POST `/api/intent/chat` | Tauri IPC `chat_turn` |
| Pipeline execution | Background job via WebSocket | Runs inline during `chat_turn` |
| Graph delivery | WebSocket `complete` event | `get_graph` IPC call after `intent_ready` |
| Project storage | PostgreSQL | Local JSON files |
| Auth | JWT tokens | No auth (single-user) |
| Export: SVG/CSV/MD | HTTP GET `/api/projects/:id/export/:fmt` | `export_content` IPC |
| Export: GNS3 | HTTP GET (ZIP download) | `export_gns3` IPC (base64 ZIP) |

---

## 6. What's not yet tested (known gaps)

| Area | Status |
|---|---|
| Juniper JunOS adapter | Phase 15 — not started |
| End-to-end pipeline with a real AI key | Manual only — no fixture tests |
| WebSocket job stream | Manual only |
| PostgreSQL DB integration tests | Manual only — no test DB fixtures |
| Tauri desktop bundle | Blocked by missing system GTK libs in this environment |
| Rate limiter under load | Unit tested; no load test harness |

---

## 7. Linting

```bash
# Rust (all non-desktop crates)
cargo clippy --workspace --exclude netgen-desktop -- -D warnings

# TypeScript / Next.js
cd netgen-web && npx tsc --noEmit
```

---

## AI provider configuration (`.env`)

| Variable | Required | Example |
|---|---|---|
| `AI_PROVIDER` | No (default: `gemini`) | `anthropic` |
| `AI_API_KEY` | **Yes** | `sk-...` |
| `AI_MODEL` | No | `claude-sonnet-4-6` |
| `AI_BASE_URL` | Only for `custom` | `http://localhost:11434/v1` |
| `JWT_SECRET` | No (insecure default) | `change-me-in-production` |
| `DATABASE_URL` | No | `postgres://localhost/labyrinth` |
| `RUST_LOG` | No | `netgen_api=debug` |

To use Anthropic Claude instead of Gemini:
```bash
AI_PROVIDER=anthropic
AI_API_KEY=sk-ant-...
AI_MODEL=claude-sonnet-4-6
```

To use Ollama (local, no API key):
```bash
AI_PROVIDER=custom
AI_BASE_URL=http://localhost:11434/v1
AI_API_KEY=ollama
AI_MODEL=llama3.1
```
