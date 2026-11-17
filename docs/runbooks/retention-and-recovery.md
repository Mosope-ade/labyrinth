# Retention, deletion and backup recovery

## Rollout and operation

1. Run migration 0016 before replacing API/worker binaries. It conservatively dates
   historical terminal jobs, preserves request identities with key fingerprints and
   installs lifecycle gates around existing worker entry points. Old renamed bypass
   functions are revoked from both runtime roles. Readiness requires this migration.
2. Deploy the API and worker from one tested image/revision. Keep migration-owner and
   recovery credentials out of both service environments. The worker's existing
   private Storage permission must support deleting the exact inventories of an
   explicitly deleted project, including its published objects. Never grant this
   capability to browser/CLI users or the API's read-only storage identity.
3. Keep the local worker running for free-tier staging cleanup. Monitor generic
   `retention_cleanup_failed` and `orphan_cleanup_failed` signals and database/storage
   capacity. Errors contain no input, object key, credential or raw SQL diagnostic.
   Service outage can delay completion; it never shortens recovery/retention minima.
4. Restore a project through its confirmed owner endpoint before the seven-day cutoff.
   Do not undo a tombstone with raw SQL or reset a terminal job. A restore keeps the
   saved head/designs; submit a new generation request to perform more work.
5. Verify idempotent delete/restore, denied non-owner access, cancellation/fencing,
   partial storage failure/retry and exact byte preservation of another project in
   disposable staging. Use synthetic designs. Published cleanup is authorized only
   by explicit owner deletion; never delete artifacts to make free-tier space.

## Application-managed backup obligations

No scheduled content-backup service has been provisioned. Before retaining real
customer content, configure and test encrypted, access-controlled exports on existing
infrastructure, with manifests recording creation/expiry and covered project IDs.
Do not purchase a backup subscription under the free-only authorization.

Every content copy must expire within 30 days; copies containing a deleted project
must also expire by its deletion request + 30 days. Replace or expire whole archives
when selective erasure cannot be verified. Rotating or copying an archive must not
restart its clock. Verify expiration for off-site copies too. Track failures and keep
receipts without recording design payloads. Do not claim this policy is enforced by
an unconfigured scheduler. Provider-managed copies require separate verification.

Supabase database exports/backups do not include object bytes. Capture compatible
DB/object manifests and test restore integrity; a database-only restore is not a
restored engineering package. Never manipulate storage.objects metadata to delete
bytes: use the existing Storage API adapter.

Maintain a current lifecycle ledger independently of content backups. Use an isolated
offline recovery connection to call `labyrinth.export_project_lifecycle(after_tenant,
after_project, limit)` with a limit up to 100; both cursors are null on the first
page. Page in tenant/project UUID order until empty. Save epochs as decimal strings.
Ledger entries contain tenant_id, project_id, epoch, deleted_at and purged, including
restored active projects with null deleted_at. Protect integrity and restrict access.
Quiesce lifecycle writes while producing a complete export. A stale backup's own
ledger cannot reveal deletions that occurred after that backup. A production-grade
continuous independent ledger/export arrangement remains a deployment acceptance
requirement; these SQL ports alone do not provide that durability.

## Restore gate — no user traffic yet

1. Stop API ingress and workers. Restore compatible DB/object backups into an isolated
   environment and apply current migrations. Never start normal services first.
2. Verify the latest independent complete ledger's provenance and completeness. If
   it is missing or untrusted, keep traffic blocked and escalate to the operator.
3. For each entry, use the recovery-only
   `labyrinth.reconcile_project_lifecycle(tenant_id,project_id,epoch,deleted_at,purged)`.
   It ignores older/replayed epochs or projects absent from this backup, creates a
   fresh fence, reapplies deletion/restore state, resets erasure receipts for repeat
   cleanup and audits the operator principal. It cannot be invoked by API/worker roles.
4. Run retention cleanup with the worker; leave ingress blocked. Verify deleted
   projects remain unreadable, in-flight jobs cannot publish, retained active designs
   have intact manifests and overdue deleted payloads/objects are erased again.
5. Verify backup expiry, Auth, private Storage, cross-tenant denial and dependency
   readiness before enabling ingress. Record tested recovery scope/time. Do not
   substitute these instructions for an actual provider restore drill.

Audit pruning remains the separate privileged tool with its 365-day minimum. This
runbook does not authorize account deletion, new backup spending or live production
purges. Database owners can bypass database protections; never use owner credentials
for normal services.

Use a separate unprivileged recovery LOGIN granted only `labyrinth_recovery` for
ledger export/reconciliation. Routine `labyrinth_maintenance` audit pruning cannot
reconcile lifecycle state. Keep recovery credentials offline and out of API, worker
and routine maintenance environments; verify role memberships before the drill.

When scoped technical identities are configured, exclude active storage_principals
data from content restoration (or restore those rows disabled before commit).
Re-provision/review technical identities and credentials in the isolated target
before enabling Storage or workers; never resurrect an old enabled flag from a
backup after an identity was revoked. Apply the reviewed provider RLS bootstrap and
verify negative scope checks alongside lifecycle-ledger reconciliation.
