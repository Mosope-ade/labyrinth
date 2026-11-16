# ADR 0008 — Admin, catalog release and audit governance

Status: accepted, 2026-11-16. The user approved operator-only administrator role
maintenance, no implicit private-topology access, 365-day minimum audit retention,
and one immutable hash per catalog semantic version.

A private registry establishes admin authority from verified application subjects;
JWT/user metadata and ordinary onboarding cannot grant it. Revocation/suspension is
checked per operation. Registry changes use separate operator credentials and audit
the database principal. Suspended users cannot receive new admin grants, and HTTP
suspension cannot target admin accounts. Application roles cannot edit the registry.

Global admins may inspect operational metadata, validate/promote generic catalog
bundles, change admission eligibility, suspend ordinary users/tenants and cancel
specified jobs with explicit current-state/target confirmation. They gain no tenant
membership and cannot access private requirements, topology bytes or artifacts via
that role. Accepted work is not implicitly cancelled by suspension; explicit audited
cancellation preserves attempt fences, retries and terminal publication history.

Catalog validation uses the pure engine schema/semantic loader. Promotion checks the
reviewed semantic SHA-256 and serializes by version; conflicting version/hash releases
reject without changing snapshots or project pins. Promotion replay never undoes a
later admission disable. Migration preflight must identify legacy duplicate versions;
never rewrite/delete referenced snapshots merely to force a unique index to pass.

Sensitive changes and audit outcomes commit together. Audits contain stable IDs,
action/category and timestamps, never YAML, prompts, credentials or exception text.
Known-user denied access is audited; unprovisioned subjects have request telemetry
without implicit user creation. Immutable application audit access is separated from
maintenance-only bounded pruning after 365 days. Retention runs audit their own
operator principal. Published engineering artifacts and job metadata are untouched.

The operational overview reports available deterministic metrics; AI/token-cost
telemetry is marked unavailable until AI integration exists. Hosted load, role
provisioning and recovery drills remain deployment acceptance work, not local-test claims.
