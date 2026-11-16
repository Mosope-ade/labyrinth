# Deployment preparation acceptance

The backend container workflow builds pinned official base images, checks non-root
configuration, runs real offline engine generation with no network/hosted credential,
checks package integrity and enforces fail-closed API startup. The provider proposal
is validated against a hash-pinned Render schema using locked wheel-only tooling.

Native local evidence: real public fixture generation under prlimit with a cleared
environment; 13-artifact inventory/input/manifest/five-pass checks; deliberate payload
corruption rejects; shell syntax and strict blueprint validation; duplicate/credential
configuration rejects without echoing values. Readiness tests cover healthy DB/role/
schema, signing-key rotation and expired-key provider outage plus closed database.

This workspace has no Docker daemon and disallows rootless Podman namespaces. A
rootless diagnostic escalation timed out in automatic review. Therefore local native
checks do not constitute a local container-image build. Remote container CI results
must be recorded separately. No hosted deployment, paid resources or live Auth/Storage
acceptance is claimed until the staging runbook is actually completed.

Local checkpoint: **191 passed, 0 failed, 0 ignored** in the locked workspace/all-features
suite. Formatting/Clippy, native bounded generation and corruption rejection, shell
syntax, provider schema validation and credential-safe rejection checks passed.
Remote container build evidence is intentionally separate from this local checkpoint.

Remote verification found a default-feature lint gap: the storage-loopback test helper
was compiled without the PostgreSQL tests that use it. Its cfg now matches that test
feature; no warning suppression was added. Both default and all-feature Clippy gates
passed locally. Checkout is pinned to the official v7.0.1 revision using Node 24,
verified from the action's release/tag and action.yml; credentials remain nonpersistent.

Verified remote head `8f9a5c91412036022573268e8165b8d43af57dd5`:

- [Backend container](https://github.com/Mosope-ade/labyrinth/actions/runs/43881543396): success.
- [Engine contracts](https://github.com/Mosope-ade/labyrinth/actions/runs/43881542288): success.
- [Backend PostgreSQL admission](https://github.com/Mosope-ade/labyrinth/actions/runs/43881542255): success.
