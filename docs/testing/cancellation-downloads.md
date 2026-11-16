# Cancellation and download acceptance

Run the locked workspace/all-features suite against the isolated PostgreSQL/pgmq
fixture described in [worker acceptance](worker-publication.md), plus formatting and
Clippy with warnings denied. No hosted credentials are required.

`api/src/http/database_tests.rs` exercises signed bearer requests through Axum and
the restricted database role. Cancellation tests cover outsider/viewer denial,
editor access, repeated cancellation and authoritative terminal status. Existing
worker tests separately cover running cancellation blocking lease renewal and
finalization, and cancelled subprocess termination.

Download tests seed synthetic published metadata as the migration owner, then use
a local HTTP storage fixture through a test-only constructor. They check returned
bytes, private authentication headers, no-store responses, no redirect URLs,
outsider denial, equal-length hash corruption, permission revocation during storage
fetch, and capacity held until response-body consumption. They do not claim that
the synthetic metadata fixture proves engine generation; the worker integration
suite covers real generation and publication separately.

Production transport configuration rejects HTTP, embedded credentials, paths,
queries, fragments and unsafe bucket names. Missing complete storage configuration
keeps admission/status available while authorized downloads report unavailable;
partially specified or malformed configuration fails startup.

Orphan cleanup is not implemented in this checkpoint: its requested safety-window
decision remains pending. Published-artifact retention and live hosted acceptance
remain separate work.

Local checkpoint: the locked workspace/all-features suite passed **173 tests,
0 failed, 0 ignored** against PostgreSQL 18.6/pgmq 1.8.1. The focused download fixture
was additionally rerun after making object keys unique across repeated test runs.
Formatting and workspace Clippy with warnings denied are required gates.
