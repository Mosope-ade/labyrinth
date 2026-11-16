# Admin operations and audit retention

Apply migrations with the separate migration owner, never API/worker credentials.
Before migration 0013, run an operator preflight:

```sql
SELECT semantic_version, count(DISTINCT sha256)
FROM labyrinth.catalogs GROUP BY semantic_version HAVING count(DISTINCT sha256)>1;
```

If conflicts exist, stop and prepare a compatibility migration. Do not delete or
rewrite published/previously referenced catalog snapshots to bypass the new release
constraint. The current repository fixtures have no such conflicts.

## Grant/revoke admin

First use normal authenticated explicit onboarding for the intended application user.
Using operator database credentials, insert their UUID into labyrinth.admin_roles.
Use a parameterized operator database client; do not paste credentials or raw tokens
into SQL/history. ON CONFLICT DO NOTHING is a safe grant replay. To revoke, delete
the corresponding registry row. Triggers record grant/revoke plus session principal;
updates are prohibited and suspended-user grants reject. API/worker roles have no
registry mutation access. There is no HTTP role-provisioning endpoint.

Use the bearer-authenticated [admin contract](../contracts/admin-v1.md) to inspect
operational metadata, validate a catalog, review its hash, then promote that exact
hash/version. Promotion never chooses devices for users or changes saved designs.
Use admission compare-and-set to enable/disable releases; archived hash snapshots
remain available for accepted jobs and published artifacts.

Suspension blocks new admission and private interactive access. It does not erase
or automatically cancel accepted jobs. Inspect state-only admin job metadata, then
confirm that job ID and expected state when cancelling work. Never reset job/attempt
history or force success to clear a stuck job. Terminal verified revisions remain intact.

## Audit retention

Provision a distinct, unprivileged LOGIN granted only labyrinth_maintenance; keep it
out of API/worker environments. Its role can execute the bounded retention function
but cannot read/update/delete tables directly. Use verified TLS for hosted access.

Run `cargo run -p labyrinth-api --example prune_audit --locked -- 1000` with
LABYRINTH_MAINTENANCE_DATABASE_URL supplied through the operator secret manager.
One invocation deletes at most 1000 records strictly older than 365 days and records
its operator principal. Repeat when needed; do not schedule it until operational
backup/retention procedures are approved. Malformed batches or recent rows reject.
No project, published revision, artifact or worker cleanup metadata is pruned.

Restore/backup drills must include both database and private object storage. These
procedures and live provider acceptance remain pending; no hosted role was provisioned
or audit data erased by the local test fixture.
