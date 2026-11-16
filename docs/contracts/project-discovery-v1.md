# Project and revision discovery V1

All endpoints require the existing Supabase bearer authentication and current active
user/tenant membership. Viewers may inspect. An outsider or suspended identity gets
403; a missing project/revision within an authorized tenant gets 404. The database
rechecks access for every request, using restricted server functions. Neither direct
table privileges nor the internal tenant authorization helper are granted to API
clients. Existing no-store, nosniff, request-ID, deadline and concurrency guards apply.

## Project listing

`GET /v1/tenants/{tenant}/projects?limit=25&after={project_uuid}`

Response: `{ "items": [...], "next_after": "uuid-or-null" }`.
Each item has project_id, name, head_revision_id (nullable), and created_at in UTC.
Only projects belonging to the authorized tenant are returned.

## Revision listing

`GET /v1/tenants/{tenant}/projects/{project}/revisions?limit=25&after={revision_uuid}`

The same page envelope contains revision_id, job_id, manifest_sha256,
catalog_sha256 and created_at for each published, succeeded revision. Historical
revisions remain discoverable even after the project head changes. Unfinished or
unsuccessful job metadata never becomes a discoverable published design.

## Revision detail

`GET /v1/tenants/{tenant}/projects/{project}/revisions/{revision}`

Returns revision_id, job_id, manifest_sha256, catalog_sha256, created_at and artifacts.
Each artifact contains name, sha256 and size_bytes. The fixed package inventory is
sorted by name. No storage keys, credentials, raw requirements or solver inputs
are included. Artifact bytes use the existing authenticated artifact download route;
this metadata response does not grant independent storage access.

## Paging semantics

The optional limit defaults to 25 and is bounded to 1–100. after is a UUID cursor,
not an offset or timestamp. Unknown/repeated query fields, malformed UUIDs and invalid
limits return 400. Results are sorted by ascending immutable UUID, **not creation
time**. The server fetches at most limit+1 rows and emits next_after only when another
row exists. The next cursor is the last returned ID; an empty final page has null.

Pages are not a multi-request snapshot. Concurrent creations with IDs before an
already consumed cursor appear when the client starts a fresh listing. Immutable
revision IDs and stable ordering avoid duplicate entries during an ordinary walk.
These contracts do not add project creation, rename, deletion or tenant provisioning.
Those operations require their own mutation contracts and acceptance tests.
