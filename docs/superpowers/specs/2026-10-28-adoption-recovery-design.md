# Adoption Recovery Design

## Intent

Resolve Toe-Tickling Issue #28: an interrupted or failed adoption can leave a subset of the target repository's guidebook files installed. The installer must provide a safe way to restore the prior state while preserving later user edits.

## Goals

- Record enough recovery information before replacing the first target file.
- Add an explicit `adopt TARGET --recover` command for a prior interrupted apply.
- Roll back only files whose current contents and permissions still match the exact output recorded by this installer.
- Preserve current user changes and retain the recovery record when an output diverges or a path becomes unsafe.
- Verify that the guidebook snapshot and digest lock return to a consistent prior pair.
- Preserve the existing preview-first, explicit-apply interface and existing target validation.

## Non-goals

- Claiming cross-file atomicity or recovery from filesystem corruption or every power-loss scenario.
- Resuming forward from a partially completed installation. Recovery rolls back to the recorded prior state.
- Overwriting a target file that changed after the journaled install, even if that prevents automatic recovery from completing.
- Changing account settings, committing to the target repository, or modifying unrelated target files.

## Current behavior

`src/adopt.rs::apply` revalidates and stages all changed files, then calls `NamedTempFile::persist` in sequence. A returned error or process interruption after an earlier replacement leaves a partial installation. `src/guide.rs` already contains in-process rollback and conflict preservation for generated outputs, but its temporary backups do not provide a durable target-install recovery command.

## Approaches considered

1. **In-process reverse rollback only.** Reuse the `src/guide.rs` pattern. This handles ordinary returned errors but cannot recover after the process exits during a later replacement, so it does not meet Issue #28's interruption requirement.
2. **Versioned per-worktree journal with explicit rollback command — selected.** Persist original file states and expected output hashes in the target's per-worktree Git metadata before staging or replacing target outputs. Recovery validates the journal and target identity, restores only exact postimages, leaves divergent files untouched, and keeps the journal until every required restore succeeds. This is the smallest approach that survives process interruption without adding user-visible transaction files to the target worktree.
3. **Documentation-only manual recovery.** This cannot reconstruct replaced user files after a partial install and may leave the mismatched snapshot/lock pair that `plan` correctly refuses. It is insufficient without a verified recovery mechanism.

## Design constraints

- Implement all new executable code and tests in Rust; use the pinned Rust 1.99.1 toolchain and the existing 2024 edition.
- Add no dependency unless implementation evidence shows an existing dependency is inadequate.
- Store one fixed journal in the target worktree's private Git directory, located with the existing sanitized Git command pattern. Do not follow symlinks while locating or using journal paths.
- Journal a version, transaction ID, canonical target path and Git-worktree identity, source repository identity, and original/new bytes and permissions for every managed path. Restrict entries to the known adoption outputs.
- Predeclare deterministic, transaction-specific staging paths in the journal. Create each stage with exclusive-create semantics and adapt it to `NamedTempFile` so replacement retains the current atomic behavior. Recovery removes only staged content that is an exact output or a prefix of its journaled output.
- Create the journal exclusively and sync it before staging or replacing any destination. Mark it committed only after all outputs have been revalidated; committed recovery removes journal/staging metadata without reverting installed files.
- On prepared recovery, treat each changed destination as one of: unchanged preimage (no action), exact expected postimage (safe to restore), or conflict (preserve and report). Revalidate every path immediately before destructive recovery operations.
- Keep a prepared journal when any conflict or restoration error remains so a later recovery attempt can continue idempotently.
- Leave newly created empty parent directories in place during rollback rather than risking removal of a user-created directory; document this harmless residue.
- Reject a new apply while a pending journal exists and direct the user to `--recover`.
- Document interruption recovery, conflict behavior, storage location, and limits in `docs/ADOPTION.md` and the CLI help.

## Acceptance conditions

- A child process can exit after one persisted output; a fresh recovery invocation restores all original bytes and permissions, including the original GUIDEBOOK/lock relationship.
- An injected later write failure returns an error and restores all unchanged-by-user outputs.
- A concurrent user edit or symlinked ancestor is preserved/rejected, never traversed or overwritten; the journal remains available.
- A completed apply is not rolled back by a later cleanup-only recovery invocation.
- Malformed, mismatched-version, wrong-target, incomplete, or out-of-scope journals produce no target writes.
- Unit tests cover each recovery state and CLI parsing; `cargo fmt`, strict Clippy, workspace tests, structural checks, and exact-head Ubuntu/Windows CI pass.
- Issue #28 stays open until the change is merged and verified on default.

## Review boundary

This design does not make an `adopt --apply` visible as one atomic filesystem change. After process interruption, rerun the explicit recovery command. If any managed output differs from both recorded states, the command reports it and preserves the user's bytes; manual resolution remains necessary.

## Source

- [Toe-Tickling Issue #28](https://github.com/Cucklord420/Toe-Tickling/issues/28)
- `src/adopt.rs` at `main` commit `d2d7c837a7d5e62ce1c1d6ee8f8a61b2edda8824`
- `src/guide.rs` at the same commit, for in-process rollback and conflict-preservation patterns
