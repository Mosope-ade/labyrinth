# Project creation acceptance

The signed-request/PostgreSQL suite covers concurrent same-key requests producing
one project, distinct requests sharing the same atomic tenant project cap, exact
replay at full quota, mismatched-key payload conflicts, current-role checks even
on replay, outsider denial, bounded/strict input and an unconfigured mutation policy.
The new project is readable through its tenant-scoped detail endpoint with no head
revision. Invalid requests leave no creation replay record.

Use the existing locked workspace/all-features, formatting and Clippy commands with
an isolated real PostgreSQL/pgmq fixture. Test policy values are fixture inputs, not
production defaults. Existing migrations/checksums are unchanged; only migration
0009 is added. No credentials, hosted tenants or projects are provisioned by tests.

Authentication onboarding is now implemented separately under the user-approved
[contract](../contracts/auth-onboarding-v1.md); see [onboarding tests](auth-onboarding.md).

Acceptance checkpoint: locked workspace/all-features suite passed **182 tests,
0 failed, 0 ignored**. Formatting and workspace Clippy with warnings denied passed.
