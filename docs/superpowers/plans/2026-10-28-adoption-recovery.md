# Adoption Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make an interrupted Toe-Tickling adoption safely recoverable without overwriting user edits.

**Architecture:** `adopt --apply` writes a versioned journal with original and expected output bytes and permissions into the target worktree's Git metadata before it replaces managed files. `adopt TARGET --recover` validates the target and journal, restores only exact installer postimages, and preserves conflicts while retaining the journal for retry.

**Tech Stack:** Rust 1.99.1, edition 2024, existing `serde`, `serde_json`, `sha2`, and `tempfile` dependencies.

**Spec:** `docs/superpowers/specs/2026-10-28-adoption-recovery-design.md` and Toe-Tickling Issue #28.

## Global Constraints

- All new executable code and tests must be Rust.
- Keep preview read-only and require explicit `--apply` to install.
- Store the versioned journal in the target worktree's per-worktree Git metadata, not tracked target files.
- Never restore over a target file that matches neither the journaled preimage nor postimage.
- Do not follow symlinks or accept journal-supplied paths outside the known managed outputs.
- Do not claim multi-file atomicity or guaranteed power-loss recovery.
- Preserve original source bytes, permissions, user edits, and existing Git-worktree validation.

## Review Focus

- Process exit after one replacement: restart recovery must return all managed outputs to their prior state.
- A concurrent edit after interruption: recovery must preserve it and retain a usable journal.
- An ancestor replaced by a symlink: recovery must refuse to traverse it and leave outside files unchanged.
- A malformed journal or inconsistent pre-install guide/lock pair: recovery must fail before target writes.
- A fully committed transaction whose cleanup was interrupted: recovery must remove only journal metadata, not revert installed files.
- A Windows read-only bit cleared without a durable matching journal transition: recovery must preserve the current permission and keep the journal for manual resolution.

---

### Task 1: Journal model and validation

**Files:**
- Modify: `src/adopt.rs`
- Test: `src/adopt.rs` unit tests

**Interfaces:**
- Add private serializable `AdoptionJournal`, `JournalEntry`, `JournalPermissions`, and `JournalState` types in `adopt.rs`.
- Add `pub fn recover(root: &Path, target: &Path) -> Result<()>`.
- Keep existing `pub fn apply(plan: &AdoptionPlan) -> Result<()>` as the install entry point.
- Each entry stores original/new bytes and permissions plus its predeclared transaction-specific staging path.

- [ ] **Step 1: Write failing journal-validation tests**

  Add tests named `recovery_rejects_wrong_target_without_writes` and `recovery_rejects_inconsistent_snapshot_lock_without_writes`. Each fixture creates a disposable Git target and an invalid prepared journal; assert the recovery error and byte-for-byte preservation of all target files.

- [ ] **Step 2: Run the focused Rust tests and confirm expected failures**

  Run `cargo test --locked adopt::tests::recovery_rejects -- --nocapture` in the isolated worktree. Expected: tests fail because journal recovery does not exist yet. If the local MSVC linker is unavailable, record that exact limitation and use the exact-host CI run for execution evidence.

- [ ] **Step 3: Implement versioned journal parsing and validation**

  Validate schema version, canonical target path, current Git-worktree identity, allowed managed paths, unique entries, preimage and postimage bytes/permissions, staging paths, and original GUIDEBOOK/lock consistency before any target write.

- [ ] **Step 4: Run the focused Rust tests**

  Run the same command; expected: both tests pass with no target mutations.

### Task 2: Durable apply and rollback recovery

**Files:**
- Modify: `src/adopt.rs`
- Test: `src/adopt.rs` unit tests

**Interfaces:**
- Add private `write_journal_exclusive`, `write_journal_update`, and `recover_prepared` helpers.
- `apply` must reject an existing prepared journal and report the exact recovery command.
- Recovery must be idempotent: preimage means no action; exact postimage means restore/remove; any other state is preserved and reported.
- Write new journals as version 3; read version 1 and 2, validate them under their prior formats, and migrate them atomically to version 3 before any target permission change.
- Persist each destination transition as `stable`, `pending`, or `attributes_cleared`; only the last state authorizes a Windows writable variant caused by clearing a recorded read-only bit.

- [ ] **Step 1: Write failing interruption and conflict tests**

  Add tests named `recovery_rolls_back_after_process_exit`, `apply_rolls_back_after_later_write_failure`, `apply_commits_all_outputs_and_cleans_recovery_metadata`, `recovery_preserves_concurrent_user_edit`, `recovery_rejects_symlinked_ancestor`, `recovery_removes_newly_installed_outputs`, `recovery_cleans_staged_files`, and `recovery_cleans_committed_journal_without_rollback`.

- [ ] **Step 2: Run the focused Rust tests and confirm the expected failures**

  Run `cargo test --locked adopt::tests::recovery_ -- --nocapture` and `cargo test --locked adopt::tests::apply_rolls_back -- --nocapture`. Expected: missing journal/recovery behavior fails. The process-exit test must launch a child test process that exits after the first persisted output so file destructors cannot simulate cleanup.

- [ ] **Step 3: Implement transaction recording and safe restore**

  Persist the journal atomically before staging/commit, stage every changed output at its journaled adjacent path before replacing any destination, check exact baseline state before each replacement, mark committed only after all outputs validate, and retain the journal whenever rollback or conflict handling is incomplete. Use `safe_path` and revalidate immediately before remove/restore operations.

- [ ] **Step 4: Run focused Rust tests**

  Run both commands again; expected: all tests pass and conflict/symlink cases preserve the target's external or user-edited bytes.

### Task 3: Recovery CLI and user instructions

**Files:**
- Modify: `src/main.rs`
- Modify: `src/adopt.rs` module documentation
- Modify: `docs/ADOPTION.md`
- Test: `src/main.rs` unit tests or Rust CLI integration test

**Interfaces:**
- `adopt TARGET --recover` invokes `adopt::recover` without planning or applying new outputs.
- Normal preview and `adopt TARGET --apply` retain their current syntax and behavior.

- [ ] **Step 1: Write failing CLI-dispatch and documentation checks**

  Add a Rust test named `adopt_recover_dispatches_to_recovery` for the exact two-argument mode and reject combined `--apply --recover`. Update the structural documentation expectation only if the checker explicitly covers CLI syntax.

- [ ] **Step 2: Run the focused CLI test and confirm failure**

  Run `cargo test --locked adopt_recover_dispatches_to_recovery -- --nocapture`. Expected: the recovery option is rejected before the parser change.

- [ ] **Step 3: Add CLI dispatch and recovery documentation**

  Document the journal's per-worktree Git metadata location, rollback semantics, conflicts, retry behavior, harmless empty-directory residue, and the limits around power loss and arbitrary concurrent writers. Refresh the reading manifest and regenerate its derived artifacts with `cargo run --locked -- refresh-manifest` followed by `cargo run --locked -- build`, then review their diff.

- [ ] **Step 4: Run the full project verification**

  Run `cargo fmt --all -- --check`, `cargo clippy --workspace --locked --all-targets -- -D warnings`, `cargo test --workspace --locked`, `cargo run --locked -- check`, `cargo run --locked -- render --check`, and `git diff --check`. Extend Windows CI to run the adoption recovery tests as well as existing guide transaction tests. Confirm exact-head push-triggered Ubuntu and Windows Actions results before creating the canonical PR.

- [ ] **Step 5: Review and deliver after the journal-transition follow-up**

  After Task 4 is complete, inspect the complete diff for path-safety, privacy, permissions, symlink, unrelated-work, and journal-recovery issues. Recheck `main` and open PRs, create only the one necessary canonical PR for Issue #28, and leave the issue open until merge and default-branch verification.

### Task 4: Journal Windows read-only transitions

**Files:**
- Modify: `src/adopt.rs`
- Modify: `docs/ADOPTION.md`
- Modify: `docs/superpowers/specs/2026-10-28-adoption-recovery-design.md`
- Modify: `docs/superpowers/plans/2026-10-28-adoption-recovery.md`
- Modify: `policy/reading-manifest.json`

**Interfaces:**
- Add a serialized per-entry destination transition state with `stable`, `pending`, and `attributes_cleared` values.
- New journals use version 3 and include the transition state in their integrity digest.
- Read version 1 and 2 journals, treat old entries as `stable`, validate them first, and atomically migrate them to version 3 before changing destination permissions.
- A writable variant of a recorded read-only destination is recoverable only when that entry is durably marked `attributes_cleared`; stage-file permission cleanup remains independent.
- Immediately before destination promotion or removal, re-read the journal and revalidate the worktree identity, safe paths, exact stage state, and authorized destination state.

- [x] **Step 1: Add the failing Windows regression**

  Add `recovery_rejects_unjournaled_windows_readonly_clear`. Create a prepared journal with a read-only GUIDEBOOK preimage, clear only that file's read-only bit without changing its bytes or the journal, call recovery, and assert it errors while preserving the writable file state and journal.

- [x] **Step 2: Run the regression and confirm the current unsafe behavior**

  Run: `cargo test --locked adopt::tests::recovery_rejects_unjournaled_windows_readonly_clear -- --nocapture` on Windows.
  Expected: FAIL because the current implementation silently restores the bit and removes the journal. The focused test failed on Windows CI at commit `06d518bd6ebf5800cff50be444654632481344e6`; the assertion observed `unwrap_err()` on `Ok(())`. The local Windows linker is unavailable.

- [x] **Step 3: Implement version-3 transition journaling and migration**

  Persist `pending` before clearing the destination bit, persist `attributes_cleared` only after rechecking the writable variant, and persist `stable` only after replacement/removal and exact state verification. Recovery must reject writable variants in `stable` or `pending`. Migrate validated version-1/version-2 journals to version 3 with stable entries before any destination permission mutation. Do not use destination transition state to authorize stage cleanup.

- [ ] **Step 4: Add transition, migration, and race regressions**

  Verify `recovery_accepts_recorded_windows_readonly_clear`, `recovery_refuses_pending_windows_readonly_clear`, and `recovery_migrates_v2_journal_before_permission_changes`; keep `recovery_restores_legacy_v1_journal` passing. Assert interrupted `pending` states preserve the target and journal, while valid `attributes_cleared` states recover and clean up. Inject a concurrent destination edit and a stage-content change after the marker is durable but before promotion; assert neither reaches the destination.

- [x] **Step 5: Update user-facing recovery documentation and manifest**

  Describe version-3 transition markers, version-1/version-2 migration, fail-closed behavior after a crash between clearing and recording the marker, manual permission restoration, and the distinction between stage cleanup and destination state. Refresh and verify the adoption-document hash in the reading manifest.

  The Rust refresh command could not build because `link.exe` is unavailable locally. The SHA-256 for `docs/ADOPTION.md` was computed directly, and all 32 manifest document hashes were checked against their current files.

- [ ] **Step 6: Run final verification and exact-head CI**

  Run formatting, focused regression tests, strict Clippy, workspace tests, structural and render checks, and `git diff --check`. Inspect exact-head Ubuntu and Windows CI before opening the single canonical PR; do not create another PR or Issue.
