# Hosted API schema verification

The generated contract covers 31 current paths / 33 operations. Its source lives in
`api/src/openapi.rs`, `api/src/openapi/operations.rs` and
`api/src/openapi/responses.rs`; engine schemas are imported with local references.

Focused checks:

```sh
cargo run -q -p labyrinth-api --example export_openapi --locked > schemas/http-v1.openapi.json
cargo test -p labyrinth-api --test openapi --locked
```

The six contract tests cover saved-export equality, unique operation IDs, router-path
coverage, typed success envelopes, admin role annotations, schema-reference
resolution, required nullable expected-head values, forbidden authority fields,
mutation confirmations, metadata private-field rejection and identifier formats.

The existing PostgreSQL HTTP fixture now validates each response passed through its
`send` method against the route/method/status schema. This exercises actual database
payloads for identity/onboarding, discovery/detail/readiness validation, job status,
diagnostics, admin metadata and mutations, including rejected operations. Formats
are checked; no runtime requests or responses are modified by this test helper.
Run the workspace integration suite with the documented disposable PostgreSQL/pgmq
setup and `LABYRINTH_TEST_DATABASE_URL`, never a production database:

```sh
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Download integrity/authorization tests remain authoritative for bytes and headers.
OpenAPI intentionally leaves downloaded JSON content and error.details extensible;
it does not prove feasibility, provider availability, or complete generated-SDK
compatibility. Unknown routes/methods are tested as HTTP errors, not enumerated as
valid paths in the contract. No new public endpoint, permission, dependency or
migration is introduced by this slice.

Local checkpoint (2026-11-16): 197 passed, 0 failed, 0 ignored in the locked
workspace/all-features suite using disposable PostgreSQL/pgmq. Both Clippy feature
configurations, formatting and diff checks passed. This is local integration
verification; remote CI and hosted provider acceptance are separate evidence.
