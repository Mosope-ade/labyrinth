# Project discovery acceptance

The API's signed-request/live PostgreSQL test suite covers:

- Three projects paged in bounded, stable UUID order without overlap.
- Cursor exhaustion and next_after=null on the last page.
- Tenant/member authorization at each read.
- Published/succeeded revision filtering and historical revision detail.
- Artifact inventory metadata without storage keys or requirement payloads.
- Unknown project/revision denial and the private tenant-authorizer ACL.

Run the same locked workspace/all-features suite, formatting and Clippy gates used
by [HTTP acceptance](authenticated-http.md). The test login needs the isolated real
PostgreSQL/pgmq fixture. Synthetic revision metadata is inserted by the migration
owner solely to exercise discovery; real generation/publication remains covered by
worker integration tests. No hosted load or deployment acceptance is claimed.

Local acceptance (2026-11-16): **176 passed, 0 failed, 0 ignored** in the locked
workspace/all-features suite against PostgreSQL 18.6/pgmq 1.8.1. Formatting and
workspace Clippy with warnings denied passed. Additional cases cover invalid/repeated
query fields, viewer access and tenant suspension. Hosted deployment remains untested.
