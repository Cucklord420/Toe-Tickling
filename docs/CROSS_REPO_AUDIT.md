# Cross-project instruction review — public, generalized findings

Date: 10-15-2026. An authenticated metadata-level inventory of accessible repositories was performed, along with representative current instruction files and selected project documentation. **This was not a full review of every historical branch, archived chat, every source file, or every private record.** Do not infer completeness beyond those inspected materials.

The following requirements recur in applicable project instruction files and have been generalized into the canonical authority, engineering, security, audit and evolution rules:

- Evidence first: inspect real source and observed behavior; separate plans, guesses, tests, and verified outcomes; prefer dated primary-source verification.
- Preserve before change: reversible changes, safe branches, no force pushes, rollback points, tests for real launchers and failure paths, and no destructive storage operation without a verified target.
- Keep private data private: avoid exporting or copying client/workplace data, credentials, personal archives, finance data or hidden infrastructure into a public guide.
- Backups must be restored in an isolated test before claiming recoverability; migration requires verified parity and exact artifact/output checks.
- Treat cross-account and provider access as capability-bound; a plan for an integration is not an installed or executed integration.
- Rust-first is a verified incremental migration preference, not a mandate to delete a working portable implementation before parity.
- Automation needs real scheduling/permissions, bounded outputs, observable results, no invented monitoring or durable memory.
- Model and artifact audits need independently verifiable evidence; a model's own statement that it complied is insufficient.

Private material and repository identities were deliberately excluded. Applicable specialized instructions should remain local to those projects, and current explicit user requests continue to govern their scope. Future reviews should incrementally expand verified coverage without leaking source details.

Open risks: provider-specific instruction-loading behavior has not been measured across every AI environment; archives and historical private rules were not exhaustively reconciled; the staged Rust checker compiled and passed its 13 tests and guidebook check on PR #2 (10-15-2026), but Rust is not a complete port and new revisions still need their own checks.
