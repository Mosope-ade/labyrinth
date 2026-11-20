# Frontend workspace V1 — screen and flow brief

Design preparation only; no frontend implementation or backend-acceptance waiver.
This turns PRD FR-17–FR-22 into screen responsibilities. Stack: React/TypeScript,
React Flow, Tailwind/shadcn and Monaco (`@monaco-editor/react`). Campus/branch WAN
is the first scenario corpus, not the product's visual or engineering boundary.

## Navigation and screen inventory

- **Sign in / session recovery:** email magic link with PKCE, pending-email, expired
  link and provider-unavailable states. Successful login inspects identity; it never
  silently creates a workspace. Browser auth remains to be implemented/tested with
  Supabase; the CLI loopback callback is not a browser GUI callback design.
- **Workspace onboarding:** explicitly create a personal workspace, then enter its
  projects. Show loading, suspended/access-denied and recoverable failure states.
- **Projects:** centered readable list with workspace context, explicit pagination,
  empty/create states and quota errors. Open a project into its latest published
  revision or an empty requirements workspace; never invent a generated preview.
  Owner deletion requires project-ID confirmation. Deleted projects show their
  recovery deadline and authorized restore action during the agreed seven-day window.
- **Project workspace:** revision selector and current job status surround the three
  panes below. Jobs/revisions remain distinguishable; a running job does not replace
  the last published package. Revisiting/reconnecting loads persisted API status.
- **Admin:** separate navigation for Operations, Catalogs, Users/Tenants and Audit.
  Hide unavailable actions in normal navigation, while enforcing access server-side.

## Three-pane engineering workspace

Left: project/site/zone tree and requirements, with a structured form/Monaco path
that works without chat. AI clarification/review becomes an optional view here.
Center: topology canvas, selection, zoom/pan, fit selection/reset layout and recorded
failure-evidence overlays. Right: context inspector for a selected node/link, or
project-wide tables/reports. Keep primary generation and job state discoverable
when either side pane is collapsed.

Inspectors cover generic archetype/mode capabilities, ports and bindings, IPAM/VLANs,
routing, firewall policy matrices, BOM, selection decisions and all five verification
passes. Selecting a counterexample highlights its actual stable entities/path.
Monaco edits input JSON/YAML and catalog drafts; generated artifacts and published
catalogs are read-only. Current API admission accepts JSON: YAML editing needs a
validated conversion boundary before submission, not an assumed YAML HTTP endpoint.

## Requirements → generation → inspection

1. Enter structured requirements or, once implemented, request an AI proposal.
2. Validate and show named missing/invalid fields inline and in an error summary.
   Display documented engine defaults separately from user values/AI assumptions.
3. For AI input, show the complete proposed values and assumptions, then require
   explicit review. A ready proposal is still awaiting confirmation; editing requires
   a new identity. See [AI contract](../contracts/ai-v1.md). This library foundation
   has no hosted review endpoint/session yet; its digest is not a browser auth token.
4. Submit through normal authenticated admission with catalog, expected revision and
   idempotency context. Display the generation options bound into the review;
   changing any option requires a new review. Normal admission still validates them.
5. Show queued/running/retry states from persisted backend evidence. Do not fabricate
   percent completion, ETAs, stages or solver activity absent from the API response.
6. For automatic model fallback, show a non-blocking, accessible popup naming the
   previous model, new model and actual switch reason. Keep the same notice in the
   chat/request history and update the active-model label. Switching does not need
   another approval click and does not imply generation or verification succeeded.
7. Show cancellation, invalid/conflicting input, infeasibility, search exhaustion,
   timeout/inconclusive execution and operational failure distinctly. Present bounded
   attempt evidence and the next supported action; never label every failure infeasible.
8. On publication, load the immutable revision/package, inspect evidence and download.
   Only backend-published verified results receive verified status. After a stale-head
   conflict refresh context and require review; do not silently replay a mutation.

## Canvas and device behavior

Dragging changes layout only. No device/link add/delete/connect action or constraint
edit mutates engineering state in V1; engineering changes return to requirements
and regeneration. Store positions, groups and viewport separately against revision
and stable entity IDs. Reject stale layout results and preserve manual positions
and viewport during job updates. Layout persistence transport is not implemented.
Evaluate the dedicated layout engine using grouped sites/zones, HA pairs, parallel
links, large labels and multi-site fixtures; move expensive work off the UI thread.

Use an original vendor-neutral vocabulary for router, switch, firewall, endpoint,
wireless and external-network nodes. Labels/symbols accompany colors; detail follows
zoom. HA pairs, link bundles and failure domains remain distinguishable. Failure
controls select recorded backend evidence; client-only graph experiments cannot
create new verification claims. Provide tree/list inspection without using the canvas.

## Admin flows and limits

Operations shows real lifecycle/failure/duration/storage metrics for stated windows;
AI tokens/costs and solver-specific charts show unavailable when not measured.
Catalogs: inspect version/hash → edit draft in Monaco → validate → review exact hash →
confirm promotion; eligibility changes use current-state/hash confirmation. Existing
revisions stay pinned. Show archetype selection frequency only when measured and
exposed by the backend. Default selection/rollback and scenario-preview UI require
matching backend capability; current validation alone is not a regression preview.
Users/Tenants: paginated operational metadata, suspension/quota state and confirmed
permitted interventions. Job inspection/cancellation preserves attempts and fencing;
the current admin API does not recreate jobs or provide unrestricted retry/recovery.
Audit is read-only. Role grants/revocations stay operator-only. Admin status grants
no automatic access to private designs, artifacts or raw prompts. See
[admin contract](../contracts/admin-v1.md).

## Viewports, interaction and unresolved visual design

Below 768 CSS px replace the app with exactly: "Designed for desktop, laptop, and tablet workspaces. Please open on a larger screen".
At 768–1023px use focus-managed overlay drawers and usable touch controls; from
1024px expose bounded resizable/collapsible panes. Ultrawide space expands the canvas,
not prose/sidebars; non-canvas/admin pages use centered readable containers. Keep
scrolling inside panels and preserve drafts/jobs across resize and reconnect.
Provide visible focus, labeled controls, non-color statuses, reduced motion, keyboard
navigation and error summaries. Scope generation/pan shortcuts away from Monaco,
inputs and browser conventions; drawer close restores focus. Long work keeps status
announced without stealing focus. Lazy-load canvas/editor views and dispose workers.

Open design decisions: original typography/color/icon treatment, exact panel/token
sizes, node detail levels, toolbar placement and layout-engine selection/benchmarks.
Browser auth/live Supabase acceptance, AI/provider/assessment screens, layout storage,
and unsupported admin metrics/actions need their backend/integration gates before
being presented as working UI. Next design artifacts are screen wireframes and the
vendor-neutral device vocabulary; frontend code still follows backend acceptance.
