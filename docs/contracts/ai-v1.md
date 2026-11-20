# Optional AI boundary V1 — provider-independent foundation

Status: contract/mock foundation; not a live AI integration. This boundary supports
PRD FR-01 and FR-14 and preserves the [AI-free CLI workflow](cli-v1.md). The engine
remains the sole generator and deterministic verifier. No AI key, provider SDK,
network call, conversation persistence, new HTTP route or frontend is introduced
by this foundation. Read-only assessment remains a future contract in this document,
not implemented behavior in this slice.

## Authority and trust

An AI response is an untrusted **proposal** of `intent_v1` partial requirements.
It cannot construct a `ResolvedIntent`, an admitted job or a verified engineering
package. Model claims that data is complete, secure, feasible or verified have no
authority. The existing engine parser/readiness checks decide which requirements
are missing or invalid. Readiness itself still does not establish feasibility.

The model may misread a prompt or fabricate a plausible value while returning
schema-valid JSON. Validation cannot establish that a natural-language extraction
is faithful. Therefore every AI-derived candidate requires explicit human review,
even when the engine reports it ready. A model-generated `confirmed` field, an
embedded instruction, an empty clarification list or a successful mock response
must never bypass that review.

The foundation is a narrow application-layer boundary. It must not import an AI
client into the engine, alter deterministic engine provenance, or make the direct
structured-JSON path depend on a provider being configured or healthy.

## Implemented library interface

The application boundary is `labyrinth_api::ai`:

- `ProviderMetadata`: supplied by the trusted adapter, not accepted from model JSON.
- `ReviewContext`: binds the actor, tenant, project, expected project head and
  catalog and the displayed generation options. Its constructor requires
  `ReviewContext::new(admission, catalog, options)`; creation does not authenticate
  or authorize anyone.
- `ReviewedProposal`: validates raw provider output and retains the owned review
  state. Its `parse(raw_output, prompt, context, provider)` constructor does not
  generate, persist, call a provider or submit a job.
- `view()`, `readiness()` and `digest()`: expose review information, exact proposal
  identity and `ProposalReadiness::{NeedsClarification(Vec<MissingField>),
  ReadyForReview}`. The projection never exposes an engine `ResolvedIntent`.
- `defaults()` and `view().engine_defaults`: read-only engine-default path/value
  entries from complete resolution only. A partial candidate returns an empty map;
  an empty map is not a promise that no defaults will apply after clarification.
- `Confirmation`: carries `schema_version: ai_confirmation_v1` and
  `proposal_sha256`; `confirm(self, &current_context, Confirmation)` consumes the
  owned review and yields `ConfirmedProposal` only for the matching content and
  current context. Matching acknowledgement/context with partial readiness returns
  `ProposalError::NeedsClarification`; only `ReadyForReview` yields a confirmed input.
- `ProposalError`, `proposal_schema()` and `confirmation_schema()`: the typed
  failures and schema exports for this boundary.

The model JSON envelope has `schema_version: ai_proposal_v1`, an `intent` containing
`intent_v1` partial requirements, and a required `assumptions` array. Each assumption
has `path` and `explanation`. The intent is retained as raw JSON and validated by
the engine before any conversion to `serde_json::Value`; parsing through an
untyped value first could discard duplicate keys. Provider/model metadata is not a
model-controlled envelope field.

The limits are 1 MiB of proposal JSON, 16 KiB of prompt text, 64 assumptions, 256
bytes per assumption path and 1,024 bytes per explanation. Assumption paths must be
unique JSON pointers to existing non-null values in the candidate intent.
Explanations remain untrusted annotations; a path match does not establish that an
assumption is true or that extraction was faithful.

## Proposal, review and confirmation

The candidate carries the existing `PartialIntent` contract; absent quantities
remain `None`, not zero, false or guessed capacities. Engine-owned secondary
defaults remain engine defaults. Human-supplied values and assumptions identified
by an LLM are still unconfirmed until review; the model cannot certify their origin.
AI extraction/assumption evidence belongs in a separate proposal sidecar rather
than new values in the engine's `Provided`/`EngineDefault` provenance enumeration.

The review boundary must distinguish these outcomes:

- Invalid proposal: malformed/oversized JSON, unknown fields, duplicate fields,
  unsupported schema or contradictory requirements; no candidate admission.
- Clarification needed: valid partial requirements and the engine's named missing
  fields; no generation merely because an AI supplied an explanation.
- Review required: a valid complete candidate remains an unconfirmed proposal.
- Reviewed input: the user explicitly confirms the exact candidate or submits
  edited structured requirements, which undergo the normal validation path again.

Confirmation binds the exact reviewed output bytes, including its envelope,
together with the review context, trusted provider metadata and a digest of the
prompt, using a versioned digest codec. Formatting changes therefore produce a
different proposal identity. An earlier confirmation cannot approve changed values.
Editing produces a new review identity; confirmation is not an instruction to merge
stale model output into newer project requirements. The digest is not an
authentication token, authorization grant or standalone persisted review receipt.
The same ordinary job-admission preconditions continue to apply: authenticated
actor, current tenant/project authorization, expected project head, idempotency key,
server-loaded catalog and engine-validated generation options. Generation options
are supplied by the application, not the model. The review context owns a canonical
JSON snapshot displayed at `view().context.generation_options`; context equality and
the `ai_review_digest_v2` codec bind that snapshot. Any changed option requires a new
review. The snapshot is limited to 64 KiB of pretty JSON, matching the existing
admission size policy. Binding is not semantic validation: options must still pass
the existing admission checks; even matching review cannot authorize unsupported
settings or prove feasibility. The shared canonical JSON helper preserves the
existing `admission_v1` identity codec and keeps object-key ordering independent of
Serde feature unification. Array order remains significant.
Confirmation never auto-submits a job. A library-level
review transition is neither authentication nor proof of a particular UI click.

`ConfirmedProposal` privately retains the reviewed sidecar; `raw_intent()`,
`review_digest()`, `context()` and `provider()` expose the corresponding input and
identity without granting admission or access.

The owned `ReviewedProposal` must remain in memory across review and confirmation.
A process restart requires a new proposal review; the hash cannot reconstruct or
restore approval. Consuming the object prevents reuse of that owned review value,
not global replay across processes or HTTP requests. There is no cross-request API,
durable AI session, auto-admission or confirmation endpoint in this foundation.
HTTP authorization, replay handling and storage retention require a later,
separately tested integration contract.

## WAN and engineering assumptions

When a prompt omits WAN connectivity, an AI adapter may propose explicit
`WanService` attachments for review. Each attachment needs site,
`provider_failure_domain`, positive `committed_bandwidth_mbps` and `connected_sites`.
The engine requires reciprocal connectivity on the same provider failure domain;
matching ISP display names never establish transport. User confirmation or editing
precedes generation. Tier3 still requires its explicit independent services and
site-resilience prerequisites; AI cannot manufacture an independence guarantee.

Capacity/security-critical omissions must remain visible. Proposed endpoint
counts, session cohorts, traffic-steering support, throughput, security rules and
redundancy assumptions cannot become authoritative merely through model wording.
The engine selects catalog archetypes using deterministic blueprint/selection rules.
An AI cannot choose an archetype as an authoritative engineering answer. Existing
revision IDs, allocation pins and core assignments must come from authorized
project context and normal pinned-update validation, not invented model references.

## Response and error boundaries

Parse bounded raw provider bytes before normalizing JSON; structured-output mode
is only a provider convenience. Reject unsupported versions, duplicate/unknown
fields and malformed or contradictory intent through the typed application/engine
boundary. Keep errors bounded and safe: do not include raw prompts, tokens,
provider credentials or complete rejected responses in logs or generic failures.
The raw prompt is not separately retained by the library; its digest is retained.
This does not make `view()` or model annotations safe for logging: annotations can
echo private prompt text and the view contains private candidate requirements.
`Debug` for `ReviewedProposal`/`ConfirmedProposal` redacts their content;
`ReviewContext` debug output omits the private generation-option snapshot. Callers
must not serialize/log their views or raw intent wholesale. Render annotations as
untrusted text, not HTML. Provider text is data; do not execute embedded commands,
follow model-selected URLs or honor instructions to modify authorization, catalog admission or verification.

Mocks exercise the application contract without external calls. They must not
masquerade as a configured production provider or report fabricated usage, cost,
latency or provider/model identity. Provider unavailability/invalid output cannot
cancel existing generation jobs, change persisted revisions or disable structured
submission and downloads.

## Future read-only post-generation assessment

Assessment is optional and applies to a completed immutable revision. Its report
must identify the exact source revision/content identity and the actual
provider/model used. It is separately labeled advisory content, not a sixth
verification pass, an engine decision, or evidence of real-device enforcement.

An assessment cannot mutate topology, BOM, addressing, policy, requirements,
verification, catalog pins or export eligibility. Suggestions require an explicit
new requirements review and the normal generation/admission path. Assessment
failure leaves the engineering package usable. A live endpoint must authenticate
and authorize the referenced private revision before loading context; an admin
role does not automatically authorize private topology access.

## Decisions required before a live adapter

The following remain open; mock behavior does not settle them:

- Gemini is the selected initial provider family. Exact model pairing, supported
  response profile and live certification remain to be completed; keep adapters
  and model configuration separate from the engine.
- Explicit consent and the minimum prompt/topology context permitted to leave the
  service, including provider data handling and assessment redaction.
- Prompt/response/conversation persistence and retention/deletion policy, if any.
- Request/response/token limits, deadlines, cancellation, retry/fallback semantics,
  concurrency/rate limits and spending caps.
- Provider credentials/configuration, usage/pricing provenance, operational failure
  classifications and safe observability.
- Authenticated proposal/review/confirmation HTTP contracts, tenant/project binding,
  stale/replayed confirmations and assessment revision authorization.

No provider is contacted and no funding or new credential is required for the
provider-independent foundation. Supabase setup remains independent work.

## Acceptance evidence required

Foundation tests must cover partial candidates → deterministic missing fields,
complete candidates → review required, exact-candidate confirmation, explicit WAN
proposal validation and preservation of the normal AI-free admission path.
Negative cases include oversized/malformed JSON, duplicate/unknown fields,
unsupported versions, invalid quantities/references, false model confirmation,
changed/stale candidate hashes and contexts, assumption limits/invalid pointers,
omitted critical fields and fabricated WAN connectivity. No unconfirmed candidate
may be presented as generated/verified, and model narrative cannot change
deterministic assurance or authorize access.

A later live adapter must additionally test provider failures, request cancellation,
configured time/token/spend bounds and retention. A later assessment implementation
must test immutable revision binding, source-content identity mismatch,
authorization and inability to mutate deterministic evidence. Those are not claimed
as completed acceptance by this library slice.

Live provider, actual retention/deletion, cross-tenant HTTP authorization and
Supabase end-to-end acceptance are separate gates. A passing mock test is not live
provider acceptance or an assessment of real network safety.


## Operational policy decisions

[ADR 0012](../decisions/0012-ai-interaction-data-policy.md) records Gemini as the
initial provider family and automatic fallback with a user-visible notification.
The backend chooses only configured, tested alternatives within the same approved
sharing scope. GUI popup/chat notice and CLI output must identify the previous and
new model and the actual reason; they do not require approval at each switch.
Remaining numeric/data recommendations stay proposed. [Gemini research](../research/gemini-v1.md)
records current models and the unpaid-tier privacy restriction. No live calls or
durable conversation storage are enabled by these decisions.
