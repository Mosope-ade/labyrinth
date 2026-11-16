# ADR 0009: policy-bound content erasure and reversible project deletion

Date: 2026-11-16. Status: accepted; implementation locally verified.

The user approved preserving published designs until explicit owner deletion,
30-day failed-job input/evidence retention, seven-day project recovery, and a 30-day
application-managed backup expiry policy with independent deletion replay. Orphan
cleanup remains at least 24 hours; admin audit history remains at least 365 days.

Use immediate project tombstones plus lifecycle epochs, rather than waiting for
object deletion to deny access or treating a restored project as the original worker
execution generation. Keep API authority checks inside restricted database functions.
Physical cleanup uses leased, bounded exact inventories and retries; it never deletes
another project's package or an active successful design for capacity management.

After policy expiry, erase payloads and raw keys while retaining minimal lifecycle,
idempotency-hash and inventory tombstones. This preserves replay/fencing protection
and safe retries without preserving topology contents. Immutable guards allow only
narrow, policy-due erasure exceptions. Existing revision/artifact identities and
mandatory audit records remain immutable. Quota release follows completed purge.

Backup restoration is an operational trust boundary: recovery-only ledger ports
reconcile latest states and invalidate stale workers before ingress opens. They do
not prove external ledger durability, configure a backup scheduler, or establish a
provider's expiry/restore guarantees. Those remain hosted acceptance gates.
