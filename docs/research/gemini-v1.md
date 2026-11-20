# Gemini V1 adapter research — 2026-11-18

Official documentation checked live on this date; not a model certification or
permission to spend/send private data. Google currently redirects Gemini API docs
from `ai.google.dev` to `aistudio.google.com/docs`. The user approved Gemini as the
provider family, notified automatic fallback and free public/synthetic testing.
The user plans a configurable paid/privacy setup for private inputs; exact models
and operational certification remain open. No provider call or service was provisioned for this work.

## Candidate pair

Recommendation for evaluation: **`gemini-3.5-flash-lite` primary**, with
**`gemini-4-hydrogen` fallback** after both pass Labyrinth's fixture/adversarial suite.
This ordering is an engineering inference: intent extraction benefits from a lighter
model; the alternate provides another capable route. It is not a measured accuracy,
latency or availability result for our prompts.

- Google's [3.5 Flash-Lite model page](https://aistudio.google.com/docs/models/gemini-3.5-flash-lite)
  identifies that model as stable, supports structured outputs, and describes simple
  extraction/document parsing among its uses.
- The [Hydrogen model page](https://aistudio.google.com/docs/models/gemini-4-hydrogen)
  likewise identifies a stable model with structured-output support. Its
  [release guide](https://aistudio.google.com/docs/whats-new-gemini-4-hydrogen)
  confirms GenerateContent and Interactions API availability. New-generation request
  settings differ: certify the actual transport/settings, not just a model string.
- Both Standard [pricing sections](https://aistudio.google.com/docs/pricing) currently
  list free input/output tokens. Published paid Standard rates per 1M tokens are
  $0.30 input/$2.50 output for 3.5 Flash-Lite and $1.50/$7.50 for Hydrogen; output
  includes thinking tokens. These are a dated research snapshot, not a spending grant
  or application cost estimate. Free access is quota-limited, not an availability SLA.
- [Current rate limits](https://aistudio.google.com/docs/rate-limits) are project-based
  and depend on model/tier; inspect actual account limits before enabling. A fallback
  cannot be assumed to escape a shared quota or provider outage.
- Do not choose the old 2.5 pair from memory: Google's
  [deprecation schedule](https://aistudio.google.com/docs/deprecations) records the
  2.5 Flash route transition. Check dates/routing again before certification.

## Structured-output constraints

Google's [structured-output documentation](https://aistudio.google.com/docs/structured-output)
exposes a JSON Schema subset. Very large/deep/branching schemas can be rejected.
It lists string-length constraints, object property counts and array uniqueness/
containment constraints as ignored during constrained decoding; array item-count
constraints are enforced. Application validation remains required for semantic values.

Create a versioned provider-wire schema compatible with the selected endpoint/model,
while preserving the complete local contract. Do not silently strip constraints from
the engine validator because a provider cannot enforce them. Test optional/null
values, references/union branches, malformed/duplicate keys, missing required fields,
assumption pointers and unknown fields through raw-byte local validation. Structured
JSON is an output format, not tool authority, user confirmation or proof of a design.

## Data handling: decision needed before real network prompts

Google's [Unpaid Services terms](https://aistudio.google.com/docs/terms#data-use-unpaid)
permit use of submitted content/responses for product/model improvement and human
review. The explicit warning is: “Do not submit sensitive, confidential, or personal
information to the Unpaid Services.” The paid-service section excludes prompt/output
use for product improvement and places that processing under its referenced data
processing terms; billing-enabled API project status determines paid-service handling.
The same terms impose paid-service conditions for clients available to EEA/Swiss/UK
users, which must be checked before public deployment.

Google's [abuse-monitoring policy](https://aistudio.google.com/docs/usage-policies)
states 55-day retention of prompts, supplied context and outputs for abuse monitoring,
with authorized review of flagged content. This is distinct from voluntary logs and
from Labyrinth's proposed 30-day chat/24-hour primary erasure targets. A local delete
cannot be represented as erasure of Google's copies within that local deadline.

Google's [logging policy](https://aistudio.google.com/docs/logs-policy) says developer
logging is default-on for Interactions calls and GenerateContent projects created on
or after 2026-10-26. Those developer logs default to 55 days before deletion marking;
configuration offers shorter windows, while datasets may outlive them. Disable
unnecessary provider logging and do not opt private data into feedback/datasets or
Contributor-tier sharing. This is separate from abuse-monitoring retention.

The [zero-data-retention guide](https://aistudio.google.com/docs/zdr) distinguishes
paid no-training handling from no-retention guarantees. Interactions stores state by
default and needs `store:false` to disable that state; grounding/file/cache features
have additional retention. Avoid those unnecessary features. Disabling logging/state
is not evidence that mandatory abuse-monitoring copies disappeared.

**Recommendation under the current zero-budget constraint:** use only synthetic or
already-public, non-personal fixtures on unpaid Gemini. Keep real private-project
parsing/assessment disabled until the user approves an appropriate paid/data-handling
arrangement and its operator limits. Redaction is useful but does not automatically
make a private topology safe to send to unpaid services. The user approved this free-testing boundary and plans a paid/privacy setup;
actual account configuration and verification remain outstanding. Model fallback
must not silently cross from a private profile to free-tier handling.

## Routing and certification boundary

Backend configuration selects eligible certified Gemini alternatives; the LLM cannot
choose a URL/provider/model. Switching automatically emits from/to/reason metadata
for a GUI popup plus persistent notice and equivalent CLI information. A transport
failure with uncertain processing cannot be turned into a second dispatch merely
because a different model is available. See [ADR 0012](../decisions/0012-ai-interaction-data-policy.md)
for approved behavior versus proposed attempt/deadline/retention limits.

Certify one primary combination first, then the alternate and switch behavior using
known provider failure semantics, raw JSON rejection, human confirmation, actual model
identity, metadata redaction and deadline/budget tests. Until then, the pair above is
recommended for evaluation, not installed, certified or guaranteed interchangeable.


## Transport implementation addendum — 2026-11-20

The library targets the current Interactions REST shape: `/v1/interactions`,
`steps` containing `model_output` text, and optional reported token usage including
thought tokens. Legacy `outputs` replies, unexpected model IDs and tool/incomplete
responses fail closed. Request configuration disables storage, tools, background
execution and streaming. These settings do not override Google's separate logging,
abuse-monitoring or account data terms.

References: [Interactions REST](https://aistudio.google.com/api-reference/interactions-api/create-interaction),
[Interactions overview](https://aistudio.google.com/docs/interactions-overview),
[structured output](https://aistudio.google.com/docs/structured-output).
Local mock acceptance is recorded in the [runbook](../runbooks/gemini-testing.md);
the evaluation pairing above still requires real extraction/model certification.
