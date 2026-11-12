# Generic device catalog

`archetypes/reference.yaml` contains synthetic generic test profiles, not vendor SKUs
or measured procurement recommendations. `schemas/` contains the generated JSON Schema.
`bundles/reference.json` pins the reference version and SHA-256 semantic content hash;
the corresponding version/hash-named YAML snapshot is retained in `bundles/`.

Loading enforces schema shape and semantic consistency. Missing throughput remains
unknown. Operating modes constrain feature combinations and known capacity. Reference
hardware hints are informational, never a device-selection key. No selection algorithm
has been implemented yet.

Run `cargo test -p labyrinth-engine` for catalog validation, schema freshness, digest
and retained-snapshot checks. See docs/contracts/engine-v1.md for units, limits, hash
semantics and the schema regeneration commands. Catalog revisions must update their
version/manifest and retain prior snapshots; never silently rewrite a released bundle.
