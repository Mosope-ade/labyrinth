# AI-free client workflow discovery V1

Authenticated tenant members can discover admitted catalog metadata and validate
structured requirements without an AI key or a pre-existing project. Validation is
read-only and does not run topology generation, select devices or prove feasibility.

- GET /v1/tenants/{tenant}/catalogs?limit=25&after={sha256}: admitted catalog version,
  hash and creation time, ascending SHA-256 cursor; limit 1–100, envelope items and
  next_after. Disabled/unadmitted snapshots are excluded.
- GET /v1/tenants/{tenant}/catalogs/{sha256}: validated generic catalog bundle JSON,
  pinned version/hash. Archived project snapshots remain available through their
  published catalog artifact rather than this current-admission discovery route.
- POST /v1/tenants/{tenant}/intent/validate: application/json body containing intent
  and optional generation options. Returns status=ready, normalized requirements,
  provenance and assurance=requirements_validated_not_generated. Critical missing
  fields return the existing 422 needs_clarification response; invalid/unsupported
  requirements/options use the existing admission error contract. Nothing is stored.
- GET /v1/tenants/{tenant}/projects/{project}/jobs?limit=25&after={job_uuid}: scoped
  job IDs, lifecycle states, creation times and published revision IDs (nullable),
  using the existing UUID page contract. No job inputs, worker tokens or object keys.

Every operation rechecks current user/tenant membership and suspension. Viewers may
read/validate; creation/admission still require owner/editor. Catalog detail returns
404 for unavailable hashes after authorization. Cursors/query fields are strict.
The existing bounded body/deadline/concurrency guards apply, and normalization shares
admission's exact readiness/options/blueprint checks. No AI integration is enabled.
