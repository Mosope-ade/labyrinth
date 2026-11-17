Source-built Labyrinth CLI evaluation packages for the hosted backend.

- Email magic-link login with S256 PKCE, same-computer browser callback, native OS
  refresh-token storage and explicit logout/workspace onboarding.
- Structured, AI-free validation/generation, project/catalog discovery, job status,
  cancellation/diagnostics and revision inspection.
- Complete package downloads checked against revision metadata and manifest hashes.
- Native platform tests, packaged-executable smoke checks, dependency notices and
  source/target/Cargo.lock provenance accompany the archives.

These are unsigned evaluation builds (Mac uses ad-hoc signing, not Developer ID or
notarization). An unlocked native credential store and a configured hosted backend
are required. Live Supabase, native-store and operational acceptance remain separate
release gates. Packaging success is not a production-readiness claim.

Verify SHA256SUMS before extraction. See the bundled README and BUILD.json, source
CLI guide and operator release runbook. No auto-updater, privileged installer,
backend credentials or offline generator is included.
