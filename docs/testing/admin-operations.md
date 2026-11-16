# Admin operation acceptance

Use the locked workspace/all-features suite with real isolated PostgreSQL/pgmq,
formatting and Clippy with warnings denied. Ephemeral signed JWTs pass the normal
verifier. Database-owner setup is limited to test fixtures.

Tests cover registry grant/revoke, forged JWT metadata denial, admin access without
private tenant membership, catalog validation, immutable version conflicts, replay
without re-enabling archived releases, accepted-job pin/replay preservation, confirmed
compare-and-set suspension, administrator-target protection, suspended-user grant
denial, append-only audit mutations and maintenance-only bounded 365-day pruning.
Operational job cancellation is tested without granting input access; synthetic
successful revision metadata remains intact after a terminal cancellation no-op.

The maintenance role cannot read audit tables or delete directly; API cannot invoke
pruning. Existing real engine/worker tests separately cover immutable publication,
resilience, cancellation/fencing and artifact preservation. AI metrics are explicitly
unavailable. No email/provider settings, hosted admin roles or live catalog promotions
are created by this local acceptance suite.

Local acceptance: **190 passed, 0 failed, 0 ignored** in the locked workspace/all-features
suite. Formatting and workspace Clippy with warnings denied passed. Actual hosted
role setup, catalog rollout, audit pruning and restore/load drills remain unperformed.
