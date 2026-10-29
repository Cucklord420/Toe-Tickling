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
- Add private `write_journal_exclusive`, `write_journal_committed`, and `recover_prepared` helpers.
- `apply` must reject an existing prepared journal and report the exact recovery command.
- Recovery must be idempotent: preimage means no action; exact postimage means restore/remove; any other state is preserved and reported.

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

- [ ] **Step 5: Review and deliver**

  Inspect the complete diff for path-safety, privacy, permissions, symlink, unrelated-work, and journal-recovery issues. Recheck `main` and open PRs, create only the one necessary canonical PR for Issue #28, and leave the issue open until merge and default-branch verification.
