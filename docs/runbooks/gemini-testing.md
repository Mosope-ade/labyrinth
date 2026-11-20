# Gemini adapter testing and profile transition

The library transport is implemented; the hosted API has **no AI route yet**.
No live provider test was run during implementation. Tests use synthetic local HTTP
servers. The core/structured CLI remain usable without a Gemini key.

## What is available

`api::ai::gemini::GeminiAdapter` accepts explicitly configured model IDs, one optional
fallback, a privacy-profile class, a key and bounded per-request settings. Production
transport is fixed to Google's HTTPS `/v1/interactions` endpoint; callers cannot
supply an arbitrary URL. It disables redirects, implicit HTTP retries, proxy inheritance, provider state,
background processing and tools. It requests the proposal schema and validates the
actual raw proposal through the existing local contract. Provider schema support
and extraction quality still require live certification for each configured model.

Current technical guards: 16 KiB prompt, 64 KiB serialized request, 2 MiB response,
1–8192 output tokens per attempt, 1–30 seconds total request time, and an explicit
one- or two-attempt allowance. These are library safety bounds, not approved global
quotas or a production SLO. Hosting must authorize current project/context and
reserve tenant/user call/token budgets before invoking the adapter.

A configured fallback uses the same client/key/profile. A failed connection before
submission or a completed, structured Gemini `NOT_FOUND`, `RESOURCE_EXHAUSTED` or
`UNAVAILABLE` availability error can select the alternate. HTTP status alone, an
opaque 5xx, ambiguous timeout/disconnect, malformed output, unexpected model,
safety/action response or invalid proposed requirements does not start a repair
loop. Respect integer `Retry-After` and protobuf `RetryInfo` delays inside the
original deadline; unsupported HTTP-date/malformed delays stop conservatively.
Both attempts consume the allowance. No claim is made that failed attempts are
unbilled. Each failure retains its attempt metadata; a switch retains from/to/reason
and a bounded message even if the alternate also fails. GUI/CLI rendering follows
when their AI routes are implemented. No new popup is installed by this library.

## Privacy profiles

- `FreeTesting` permits only caller-classified public/synthetic requests and rejects
  `PrivateProject` before HTTP dispatch.
- `PaidPrivate` represents an operator-selected privacy profile. It does **not**
  activate billing or verify that the key actually belongs to the required Google
  billing/privacy setup. Verify that account/project independently before private use.
- Classification is trusted application context, not model output or a client-controlled
  permission and not an automatic detector of confidential text. The hosted layer
  must enforce it; no hosted/private AI feature is enabled here.

The reviewed proposal records the configured profile in trusted provider metadata,
which participates in the review identity. Change models/profile through new server
configuration, keeping the engine unchanged. Active requests retain their original
configuration; automatic fallback cannot downgrade private requests to free handling.
Account-key changes and eligibility/revocation require the future hosted configuration
and authorization layer. A profile label is not proof of provider billing or privacy.
See [Gemini research](../research/gemini-v1.md) and [ADR 0012](../decisions/0012-ai-interaction-data-policy.md).

## Optional manual public smoke

This developer example sends a **fixed, fictional public prompt only**. It does not
read project files, accept arbitrary prompts, connect to Supabase, generate/admit a
network or confirm a proposal. Run it only after verifying the actual provider
account, quotas and applicable data terms. A `FreeTesting` label cannot guarantee
zero charges if the supplied key belongs to a billing-enabled project.

1. Review the constant prompt in `api/examples/gemini_public_fixture.rs` and the
   provider research. Create/configure the Google project/key yourself. Keep it local.
2. Build before putting the key in the process environment:

   ```sh
   cargo build --locked -p labyrinth-api --example gemini_public_fixture
   ```

3. In **bash** (start `bash` first if necessary), configure a public test. Model IDs
   below are dated evaluation candidates, not a claim of live certification:

   ```bash
   export LABYRINTH_GEMINI_PRIMARY_MODEL=gemini-3.5-flash-lite
   export LABYRINTH_GEMINI_FALLBACK_MODEL=gemini-4-hydrogen
   export LABYRINTH_GEMINI_PUBLIC_TEST_ACK=public-fixture-only
   read -rs -p 'Gemini API key: ' LABYRINTH_GEMINI_API_KEY
   export LABYRINTH_GEMINI_API_KEY
   printf '\n'
   ./target/debug/examples/gemini_public_fixture
   unset LABYRINTH_GEMINI_API_KEY LABYRINTH_GEMINI_PUBLIC_TEST_ACK
   ```

4. Expect `assurance: unconfirmed_public_fixture_not_generated`, engine clarification
   or ready-for-review, actual model/profile metadata, optional switch notice and
   measured token fields when reported. Missing usage is unknown, never fabricated
   zero/cost. Check extracted values against the constant prompt yourself. Invalid
   output fails; it is not silently repaired or passed to generation.

Never send the key through chat, place it in a URL, commit it, or use this developer
example as the hosted CLI login flow. The first live test remains an operator action.
No real provider traffic, private-data permission or spending was performed by CI.

## Automated acceptance

```sh
cargo test --locked -p labyrinth-api --lib ai::gemini
cargo test --locked -p labyrinth-api --test ai_proposals --test ai_review_identity
```

Coverage includes availability fallback/notifications, bounded attempts/deadline,
backoff, cancellation, free/private separation, profile-bound review identity,
credential/redirect protection, declared/chunked response caps, raw JSON rejection,
unexpected/tool/incomplete responses, and confirmation followed by existing normal
preparation. Account verification, extraction benchmarks, global budgets, authenticated
AI endpoints, conversation persistence/retention and live model certification remain
separate gates. Do not use a passing mock suite as provider approval or a privacy
certification.
