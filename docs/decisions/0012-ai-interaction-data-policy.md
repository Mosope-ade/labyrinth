# ADR 0012: AI interaction, context, retention and retry policy

Date: 2026-11-18
Status: **PARTIALLY DECIDED — Gemini, notified automatic fallback, free public/synthetic
testing and configurable future paid/privacy profiles approved; numeric policies remain proposed**
Scope: future hosted AI parsing/clarification and read-only assessment.

## Context

The user selected the **Gemini provider family for now** and approved automatic
model fallback with notification rather than per-switch approval. The user plans a paid/privacy setup for private inputs and approved free testing
with public/synthetic non-sensitive examples. Model/profile selection must remain
configurable without engine changes. Exact model IDs and the remaining numerical
retention/deployment policies are not yet approved. Earlier
no-history/no-retry suggestions were not accepted. The current
[AI foundation](../contracts/ai-v1.md) is an in-memory validation/review library,
not a hosted conversation service. This proposal does not authorize calls, create
storage, provision services or change accepted engine/retention contracts.

## Approved development and privacy profiles

Free testing uses only synthetic/public, non-sensitive examples. Private-project AI
stays disabled until an appropriate paid/privacy profile is configured and verified.
Switching model or profile is configuration, not an engine rewrite. Automatic
fallback remains within the active approved privacy profile and cannot silently
move a private request to free-tier data handling. Selecting a paid profile is not
proof of billing status; operators must verify the actual provider project/account.
No billing activation or spending is authorized by this decision.

## Recommended interaction and context

- A request requires an explicit **Send** or equivalent intentional action for each
  chat turn. Never call the model on typing, page open, resize or ordinary generation.
- Show what will be shared: the current authorized project's relevant draft plus
  bounded relevant recent turns. Enforce configured turn/byte/token ceilings; do
  not send unlimited history, other projects, credentials or unrelated artifacts.
- Recheck current project authorization before loading/sending context. Treat
  prompts, prior messages and model output as untrusted content, not instructions
  that can change privileges, catalog authority or verification rules.
- Assessment is a separate explicit action on an authorized immutable revision.
  Show its revision and redacted/minimized context before requesting permission to
  send it. Permission to parse a prompt is not blanket permission to send a package.
- All parsed candidates require review/confirmation. The engine alone generates,
  selects devices and verifies; no model tools execute commands, mutate projects,
  invoke generation, follow selected URLs or access storage directly.

## Recommended data lifecycle

- Retain user-visible project chat and parsed proposal drafts for **30 elapsed days
  since the session's last intentional user activity**. Polling/background refresh
  must not extend retention. Active-session renewal extends the window, so session
  storage quotas and an accessible delete control are also required.
- Store only application conversation/proposal content needed for that feature.
  Do not persist raw provider HTTP payloads, credentials, request headers or bearer
  tokens in the database, artifacts or logs. Bounded transient response bytes may
  be used in memory for strict validation; user-visible responses are distinct from
  the raw provider transport envelope.
- Confirmed requirements admitted to a project follow the existing project/revision
  lifetime policy. Expiring chat must not remove or silently alter saved designs,
  their catalog pins or required deterministic reproducibility inputs.
- Explicit chat/session deletion immediately revokes access and targets physical
  purge of its application content within **24 hours**. This is a proposed primary-
  data service target requiring a continuously available cleanup path and tested
  deletion fencing/recovery; it is not an implemented or provider-wide guarantee.
- Deleting the entire project retains the existing immediate access revocation and
  **seven-day recovery** policy, followed by its existing purge process. Chat-only
  deletion does not create a new recovery window or override project deletion.
- Backup/provider copies follow [existing backup policy](../contracts/retention-v1.md)
  and separately verified provider limits. A primary-data deadline does not promise
  erasure from provider retention/backups within 24 hours. Reapply deletion state
  before restored data becomes accessible.
- Retain metadata-only AI usage for **90 elapsed days**: tenant/project/user IDs,
  actual provider/model, tokens when available, latency, status and attempt count.
  No raw text. Existing mandatory admin audit retention remains separate. Global
  admin status must not grant private conversation/design access.

## Recommended request and failure policy

- A logical AI request has a configurable **30-second total deadline**, including
  any retry/backoff, and must fit its enclosing API deadline. Cancellation stops
  further dispatch; it cannot promise that a provider already processing has stopped.
- Configure enforceable per-user and per-tenant call/token limits and attempt budgets
  before enabling live/paid requests. Missing limits mean disabled, not unlimited.
  Proposed attempt cap: at most one additional attempt overall, used by either a
  same-model retry or a model switch, never both.
  Provider pricing/cost is displayed as unavailable when not reliably known; token
  counts and estimated spend must name their provider/model/pricing provenance.
- Permit **at most one automatic retry** only for a verified before-dispatch connect
  failure, documented rejection before processing, or a certified provider's actual
  idempotency guarantee. A 429/503 status by itself does not prove no processing.
  Respect `Retry-After`, the original deadline and remaining call/token budget;
  otherwise return the failure without retry. Never recursively retry the retry.
- Do not automatically resend after an ambiguous post-dispatch timeout/disconnect,
  blanket 5xx response, invalid JSON or failed semantic validation. A manual retry
  is an explicit new user action and warns when prior processing/cost is unknown.
  No automatic model-based "repair" loop turns invalid output into extra calls.
- **Approved routing behavior:** when the current model is unavailable or a switch
  is otherwise required, the backend automatically chooses a configured, certified
  alternate inside the approved Gemini provider family. No per-switch user approval
  is required. The switch must preserve existing context permissions, request budget
  and deadline; an unavailable eligible alternate produces an honest failure.
- Emit a model-switch event with actual `from`, `to` and a truthful bounded reason.
  The GUI presents it as a popup and retains a visible notice; CLI users receive
  the same model/reason information. Report the actual responding model and retry
  status. Do not invent provider explanations when the reason is unknown.
- Fallback must not duplicate an ambiguous in-flight request without actual provider
  idempotency proof. The proposed shared additional-attempt cap applies across retry
  and switch. No silent cross-provider sharing, model-selected routes, recursive
  fallback chains or automatic safety/validation-bypass loops are introduced.

The retry rule follows the distinction for non-idempotent requests in
[RFC 9110 §9.2.2](https://www.rfc-editor.org/rfc/rfc9110.html#section-9.2.2).
Treating model content as untrusted, validating deterministically and keeping model
privileges minimal follows [OWASP LLM01 guidance](https://genai.owasp.org/llmrisk/llm01-prompt-injection/).
The numeric deadlines and retention periods above are our recommendations, not
requirements imposed by either source.

## Portability, tradeoffs and approval gate

Use a model-independent application contract with provider adapters, but certify
**one provider/model combination first** against malformed-output, clarification,
confirmation, outage, context-leakage and cost/retry tests. Additional combinations
need the same acceptance evidence; an adapter interface is not an "any model" claim.
The Gemini provider family is approved; exact model IDs still require selection
and certification. See [Gemini research](../research/gemini-v1.md).

Thirty-day chat history supports continuity at a storage/privacy cost; limited
context reduces cost/exposure but can require the user to restate older facts.
Safe bounded retry improves transient recovery while refusing ambiguous automatic
resends; manual retries can still incur duplicate processing/cost. Deletion targets
require operational capacity, particularly under the free/local-worker deployment.
Before enabling calls, settle the remaining data/deployment/numeric policies and
define session authorization/storage schemas, deletion/restore fencing, adapter
limits and tests. Only the provider-family and notified automatic-fallback choices
are approved here; the other recommendations remain proposed. The current
provider-independent foundation stays non-networked.
