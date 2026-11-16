# Authenticated job HTTP API — V1

Authentication semantics: [ADR 0005](../decisions/0005-authenticated-job-http.md).
Persistence/atomicity: [job lifecycle](hosted-jobs-v1.md).

## Common request and response rules

Supply exactly one `Authorization: Bearer <Supabase user access token>` header.
No actor, admin role, session cookie, query token or service key substitutes for it.
The signed actor must also have a provisioned application user and tenant membership.
All tenant members can read project job status; owner/editor members can submit.
User/tenant suspension and membership changes are enforced at database access time.

Business responses include `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`
and a generated `X-Request-Id`. Errors are JSON:

```json
{"error":{"code":"stale_head","message":"Project head does not match the precondition"}}
```

Optional `details.fields` supplies clarification path/reason entries; `details.issues`
supplies core validation code/path/message entries. Database/JWT internals, tokens and
other tenants' payloads never appear in the public error body. Provisioning is explicit:
signing in does not automatically create a tenant, membership or admin permission.

## Submit

`POST /v1/tenants/{tenant_uuid}/projects/{project_uuid}/jobs`

Required: `Content-Type: application/json` and one `Idempotency-Key` containing 1–128
ASCII letters/digits or `._:-`. A key is scoped to tenant/actor/generate_v1, not merely
the URL. JSON rejects unknown/duplicate envelope fields and duplicate typed intent
fields. Total body limit is 2 MiB; core intent is independently limited to 1 MiB,
normalized requirements/provenance to 1 MiB each, options to 64 KiB.

```json
{
  "intent": {"schema_version":"intent_v1","name":"Supply complete structured intent here"},
  "catalog_sha256":"<64 lowercase hex characters from an admitted catalog>",
  "expected_head_revision_id": null
}
```

The abbreviated intent above illustrates the envelope and needs clarification; use a
complete [intent fixture](../../tests/fixtures/intents/enterprise.json) for valid input.
Optional `options` uses the [generation options schema](../../schemas/generation-options-v1.schema.json)
and defaults to the engine's documented options. The head field is **required**:
`null` asserts an empty project; a UUID asserts the exact current head. An incremental
intent's core base ID must be `rev-<canonical UUID>` matching that head. The base
manifest digest is loaded from authorized database metadata, not provided by the client.

`202 Accepted` for a new durable queued job:

```json
{"job_id":"<uuid>","state":"queued","replayed":false}
```

`Location` points to its relative status URL. Identical retries return `200 OK`, the
same job ID and its current state with `replayed:true`, even after a project head
advance. Different input under the same key returns 409. Default/provided provenance
is part of identity. Authorized catalog snapshots remain readable for replay, but
new admission with a disabled catalog is refused by the transaction.

No network generation or AI call runs in this endpoint. Acceptance means durable
admission, not engineering feasibility or verified completion. A timeout, disconnection
or transient error can have an unknown commit outcome: reuse the SAME key and body.

## Status

`GET /v1/tenants/{tenant_uuid}/projects/{project_uuid}/jobs/{job_uuid}`

`200 OK` returns authoritative persisted state:

```json
{"job_id":"<uuid>","state":"queued","created_at":"2026-11-15T18:00:00.000000Z","revision":null}
```

States: `queued`, `running`, `retry_wait`, `cancel_requested`, `cancelled`, `failed`,
`succeeded`. Only succeeded jobs expose a `revision` object containing `revision_id`
(UUID) and `manifest_sha256`. No progress percentage, diagnostics or artifact download
is fabricated. A succeeded row missing publication metadata returns a storage error.
Polling does not require the same actor who submitted, but does require current
membership in the owning tenant. It never returns raw requirements or object keys.

## Error codes

- 400 `invalid_request`: malformed envelope/UUID, missing head/key, duplicate headers.
- 401 `unauthorized`: missing/invalid user access token; includes `WWW-Authenticate: Bearer`.
- 403 `forbidden`: no current tenant permission, suspended user/tenant or viewer write.
- 404 `not_found`: absent project/job within the authorized tenant/project scope.
- 405 `method_not_allowed`: unsupported business-route method.
- 409 `stale_head`, `idempotency_conflict`, `invalid_base`.
- 413 `payload_too_large`: total body admission bound.
- 415 `unsupported_media_type`: JSON content type required.
- 422 `invalid_requirements`, `needs_clarification`, `catalog_unavailable`.
- 429 `quota_exceeded`: tenant active-job quota exhausted; no new job or queue message.
- 503 `authentication_unavailable`, `database_unavailable`, `server_busy`.
- 504 `request_timeout`: HTTP deadline; retry POST with its original key.
- 500 `internal_error`: unexpected processing failure, with no internal details.

A forbidden tenant response does not distinguish whether an inaccessible tenant exists.
An unknown job and a job in another project both return 404 in an authorized scope.
Errors are not cacheable. Exact frontend CORS origins, login/refresh flows and scripted
credentials are outside this interface slice.

## Liveness and server

`GET /health/live` is public and returns 204 with no private data. It is process
liveness, not database/JWKS readiness or proof of worker availability. Render's ingress
must enforce HTTPS. Run `cargo run -p labyrinth-api --locked` after configuring
[the API environment](../../api/.env.example) and applying migrations as their owner.
The binary never migrates or provisions data using runtime credentials.

## Cancellation and artifact downloads — 2026-11-16

`POST /v1/tenants/{tenant}/projects/{project}/jobs/{job}/cancel` requires current
owner/editor membership and a valid bearer token. No request payload or idempotency
key is needed. Returns `200 {"job_id":"...","state":"..."}`. Queued/retry-wait jobs
become cancelled immediately; running jobs become cancel_requested and cannot
publish. Workers stop at heartbeat/lease detection and queue recovery completes
cancellation. Repeating cancellation preserves terminal state, including succeeded
history. Cancellation and publication serialize on the job; whichever commits first
wins. Queue acknowledgment is left to redelivery to avoid lock inversion.

`GET /v1/tenants/{tenant}/projects/{project}/revisions/{revision}/artifacts/{name}`
requires current tenant membership (including viewers). Only published, succeeded
revision metadata authorizes downloads. Unknown revision/file yields 404 after
project authorization; forbidden tenant access yields 403. Supported artifact names
are the fixed core package inventory, including manifest.json. Each response is one
JSON attachment; assembling a package directory/ZIP is client work for now.

The API fetches the private object server-side, bounds it to its authoritative size
(maximum 32 MiB), checks SHA-256, and rechecks current authorization before returning
bytes. No object key, credential or signed URL is returned. Response headers include
Content-Type application/json, attachment filename, SHA-256 ETag, no-store and
nosniff. Partial/corrupt/unavailable storage returns a redacted 503
artifact_unavailable. Request capacity remains held while the response body is
consumed or dropped, protecting against slow-client memory amplification.

Configure the optional complete group LABYRINTH_ARTIFACT_STORAGE_ORIGIN,
LABYRINTH_ARTIFACT_STORAGE_BUCKET and LABYRINTH_ARTIFACT_STORAGE_TOKEN to enable
transport. The origin must be HTTPS with no credentials/path/query/fragment;
redirects are forbidden. Partial configuration fails startup. An absent group allows
admission/status service to run, with authorized downloads returning 503. Provision
a private bucket and a server-only read credential separately; never use a public
bucket or expose worker credentials to clients. End-to-end HTTP tests use a test-only
loopback storage fixture, not a production plaintext configuration switch.

Project listing, published revision listing and artifact-inventory metadata are now
available through [project discovery V1](project-discovery-v1.md). These reads share
the existing bearer authentication, current tenant authorization and HTTP guards.

Existing tenants can use [project creation V1](project-creation-v1.md) with an explicit
project-cap policy. Project creation is a separate idempotent transaction; signing
in still does not provision a tenant or membership.

Explicit [personal-workspace onboarding](auth-onboarding-v1.md) is now supported after
provider login. Signing in alone still creates nothing; a deliberate authenticated
POST provisions the application user, tenant and owner membership atomically.

[Attempt diagnostics V1](job-diagnostics-v1.md) exposes authenticated bounded attempt
history, safe failure categories and retry timing without private runtime metadata.
An inconclusive diagnostic is distinct from the job lifecycle's failed state.

[Client workflow discovery](client-workflow-v1.md) adds admitted-catalog browsing,
shared structured-intent validation/clarification and bounded job listing.

Deployment readiness: unauthenticated GET /health/ready returns 204 only while the
restricted API database role, required schema functions and trusted signing keys are
usable; otherwise 503 with no operational body. It shares bounded capacity/deadlines
and cache-coalesced key refresh. GET /health/live remains process-only liveness.
