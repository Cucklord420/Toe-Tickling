# Issue #39: Race-safe adoption and recovery implementation plan

> **For implementation:** Work task by task. Keep mutations closed until each platform's safety gate and regressions pass.

**Goal:** Replace pathname-based adoption installation and recovery with a Rust mutation context that prevents other local users from redirecting writes outside the selected Git checkout, and that fails closed when the platform cannot prove the boundary.

**Architecture:** Build a capability context over the target tree and its worktree-specific Git metadata using exact-pinned `cap-std`/`cap-fs-ext` APIs for handle-relative access. Every read used to authorize a mutation and every create, write, permission update, rename, restore, cleanup, and journal operation goes through opened directory/file handles. Linux checks ownership and mode protections along each path; Windows uses local-volume handles, one-component relative opens, no-delete sharing, and a conservative DACL trustee policy; macOS proceeds only when ACL state can be classified safely. The public `--apply` and `--recover` routes remain paused until the matching regression suite passes.

**Base:** `db2146f781e9c088d3694cfb77bd5434ec90839a` (`main`, fetched 11-07-2026).

**Spec:** `docs/superpowers/specs/2026-11-07-issue-39-race-safe-adoption-design.md`.

## Guardrails

- All new executable code and tests are Rust. Keep dependencies exact-pinned and update `Cargo.lock` using Cargo.
- Preserve journal versions 1 through 3, checksums, transition markers, conflict preservation, user edits, and retryable recovery behavior.
- Preview remains read-only. Keep `apply` and `recover` fail-closed before planning, target inspection, journal inspection, or success JSON whenever the platform guard cannot establish safety.
- Do not use repeated pathname checks as the race defense. No mutation may fall back to path-based operations after a handle guard fails.
- POSIX permits only non-root execution when each ancestry component prevents replacement by another UID; evaluate opened-object metadata and sticky-directory child ownership. macOS ACL ambiguity fails closed.
- Windows is local-volume only. Use one-component directory-handle-relative opens with reparse processing disabled, inspect each returned handle, hold directory handles without delete sharing, and enforce the scoped DACL policy from the spec. Existing path ancestry may grant only `FILE_ADD_FILE` or `FILE_ADD_SUBDIRECTORY`; it must deny deletion/replacement, ACL/owner changes, writes, generic rights, and unknown ACEs. The target/Git roots, output parents, files, and new objects stay strict and deny every untrusted mutation right, including creation. Avoid undocumented flag combinations; any unknown ACE, trustee, owner, generic right, or descriptor state fails closed.
- Newly created directories, stage files, rollback stages, and journal temporary files must have restrictive security at creation and pass handle-based verification before content is written.
- Tests use disposable Git repositories, synthetic bytes, and outside sentinels. Never use the actual home repository or personal archive data.
- Do not merge, close Issue #39, alter protections, or claim support until independent review, exact-head CI, and default-branch verification are complete.

## Task 1: Close public mutations before inspection

**Files:** `src/adopt.rs`, `src/main.rs`, `tests/adoption_pause.rs`, `src/adopt_transaction_tests.rs`.

- [ ] Add Rust CLI/API regressions proving unsupported `apply` and `recover` fail before target planning, journal reads, file writes, stage cleanup, or success JSON.
- [ ] Add malformed-target and malformed-journal cases with sentinels; assert exact byte and directory-entry preservation.
- [ ] Confirm these tests fail against current `recover` behavior, then add the narrow early fail-closed gate for both public mutations. Keep private transaction helpers testable for the platform-specific implementation.
- [ ] Run focused tests on the exact current head. Record the host/linker limitation if the local executable cannot link; never call static inspection a passing runtime test.

## Task 2: Implement POSIX capability operations and ancestry policy

**Files:** `Cargo.toml`, `Cargo.lock`, `src/adopt.rs` or a new Rust-only `src/adopt/mutation.rs`, and Rust unit tests.

- [ ] Add exact-pinned `cap-std = 4.0.3` and `cap-fs-ext = 4.0.3` for descriptor-relative open/create/read/write/rename/unlink and metadata operations. Use the no-follow capability APIs and one-component names relative to held directory descriptors.
- [ ] Build target and Git-metadata chains from a trusted filesystem root, retain every descriptor, and verify identity/ownership from opened descriptors. Reject traversal, links, special files, root execution, group/world-writable non-sticky ancestry, and unsafe sticky-parent ownership.
- [ ] Add read-only guards for ancestry and destination parents, including linked-worktree Git metadata outside the target root.
- [ ] Add test-first cases for root, unprotected writable ancestry, symlink swaps, file/directory replacement, and outside sentinels. Use a safe owned temp directory for the supported path and verify install plus recover through descriptor-relative operations.
- [ ] Add an adversarial different-UID race regression where the CI environment supports creating a second UID; otherwise test the rejection boundary and retain that race as an explicit CI limitation.

## Task 3: Implement Windows handle and DACL protections

**Files:** `Cargo.toml`, `Cargo.lock`, `src/adopt.rs` or `src/adopt/mutation.rs`, Windows-only Rust tests.

- [ ] Add the minimal exact-pinned `windows-sys = 0.61.2` features needed for handle-derived owner/DACL checks, token SID inspection, local-volume identification, and restrictive security descriptors. Reuse cap-std for handle-relative create/read/write/rename/unlink operations instead of adding a second path layer.
- [ ] Open each single component relative to its verified parent handle using the documented `RootDirectory` form and reparse-point protection. Verify every returned handle; reject any reparse point, non-directory parent, remote/unknown volume, or unsupported filesystem.
- [ ] Omit `FILE_SHARE_DELETE` from ancestry directory handles and retain them for the entire transaction.
- [ ] Inspect owner and DACL through each opened handle. Permit mutation-capable ACEs only for the current SID, SYSTEM, or built-in Administrators, except for the exact create-child-only rights allowed on pre-existing path ancestry by the amended spec. Keep that exception out of target/Git roots and operational output parents. Reject null/unreadable DACLs, broad/unresolved trustees, generic or unknown mutation ACEs, ambiguous owner/ACL state, and inherited grants that could delete or replace protected objects. Never rely only on `AccessCheck` for the current token.
- [ ] Create output directories, stages, rollback stages, and journal temporaries with restrictive non-inheriting security attributes; inspect the opened object and DACL before writing bytes.
- [ ] Add failing tests for reparse ancestors, create-only ancestry acceptance, ancestry delete/replace denial, broad and inherited Users write grants, unsafe destination/journal ACLs, unsafe newly created stage ACLs, held-handle rename denial, concurrent child replacement, and outside sentinel preservation. Verify target/Git roots and output parents still reject untrusted creation rights.

## Task 4: Fail closed on macOS ACL ambiguity

**Files:** platform-specific Rust guard and tests; `.github/workflows/validate.yml`.

- [ ] Inspect ownership/mode plus macOS ACL entries through opened handles or another documented handle-safe API. Reject named grants, unsupported flags, unknown ACL state, and ACL query errors.
- [ ] If a complete ACL classification cannot be implemented and exercised, keep macOS `apply` and `recover` paused; do not infer safety from mode bits alone.
- [ ] Add a macOS CI job for safe ACL, unsafe named grant, and query-failure regressions. Assert failure leaves the target, stage, journal, and outside sentinel untouched.

## Task 5: Route the full transaction through the guard

**Files:** `src/adopt.rs`, `src/adopt_transaction_tests.rs`, CLI tests.

- [ ] Convert journal lookup, read, initial creation, update, migration, and removal to the mutation context. Preserve the current on-disk path and versions 1/2/3.
- [ ] Convert output reads, parent creation, stage creation, permissions, final promotion, rollback, committed cleanup, and recovery cleanup to handle-relative operations. Validate journal bytes/state and file identity through the same opened objects used for the consequential operation.
- [ ] Keep the complete context and ancestry handles alive throughout the transaction. Reopen/revalidate children relative to held parents before each write/rename/delete; fail closed on identity or state mismatch.
- [ ] Extend existing regression cases so interruptions, concurrent edits, partial stages, legacy migrations, Windows permission transitions, and retryable recovery remain unchanged.
- [ ] Add synchronized race tests at each consequential boundary: before journal commit, before destination replacement/removal, and before stage/journal cleanup. The competing process must use a different identity where supported; assert no outside object changes.
- [ ] Only after the platform-specific suite passes, enable that platform's public `apply`/`recover` routes. Keep unsupported platforms paused.

## Task 6: Verification, review, and delivery

- [ ] Run `cargo fmt --all -- --check`, `cargo clippy --workspace --locked --all-targets -- -D warnings`, `cargo test --workspace --locked`, `cargo run --locked -- check`, `cargo run --locked -- render --check`, and `git diff --check`.
- [ ] Run platform-specific race and failure regressions on Ubuntu, Windows, and macOS. Use exact-head CI if local Windows or macOS link/runtime tools are unavailable; report every blocked path.
- [ ] Inspect the complete diff, dependency changes, license headers, workflows, journal compatibility, privacy, and rollback. Run a fresh independent review after the implementation is complete.
- [ ] Refresh the canonical default branch and check for an existing matching PR. If none exists, open only one Draft PR linked to Issue #39 after the exact-head checks pass; do not create a duplicate tracker.
- [ ] Keep Issue #39 open and platform mutations disabled until review, merge authorization, exact default-branch CI, and post-merge verification. Use a reviewed revert as rollback.
- [ ] Update the private audit receipt with actual evidence. Do not commit it or claim it passes while any required check is blocked.
