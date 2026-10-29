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
- Journal a version, transaction ID, canonical target path and Git-worktree identity, source repository identity, and original/new bytes and permissions for every planned output. Restrict entries to the known adoption outputs, require the core GUIDEBOOK/lock/AGENTS/ignore paths, and store an exact planned-path manifest that must match the entries.
- Predeclare deterministic, transaction-specific staging paths in the journal and create each stage with exclusive-create semantics. Promote adjacent stages with `std::fs::rename`. On Windows, before clearing a read-only destination, recheck its exact journaled state and durably mark that entry `pending`; after clearing and verifying the bit, durably mark it `attributes_cleared`. Only `attributes_cleared` authorizes recovery to accept a destination whose bytes match a recorded state and whose only difference is the cleared read-only bit. Immediately before each promotion or removal, re-read the journal and revalidate worktree identity, safe destination/stage paths, exact staged bytes and permissions, and the destination's expected or journal-authorized writable state. If any state changed after the permission marker was written, preserve it and keep the recovery journal. After replacement or removal and verification of the expected bytes and permissions, return the entry to `stable`. Clearing an installer-owned stage is a separate operation and never authorizes a writable destination variant.
- Continue reading pending version-1 and version-2 journals at the stable Git-metadata path. Validate each using its own schema, identity, allowed paths, and integrity rules, then atomically migrate it to version 3 with every entry initially `stable` before any target permission change. Version-1 journals predate the checksum and planned-path manifest; version-2 journals include both.
- Hash the normalized version-3 journal payload, including per-entry transition states, with SHA-256 and verify it before any recovery write. This catches accidental single-file edits or corruption but is not a keyed authenticity check.
- If a crash occurs after the destination's read-only bit is cleared but before `attributes_cleared` is durably recorded, keep the journal and fail closed when recovery sees the writable variant. The user must inspect the target and restore its recorded permission before retrying. A writable variant without the matching journal state is treated as a concurrent user change and preserved.
- Create the journal exclusively and sync it before staging or replacing any destination. Mark it committed only after all outputs have been revalidated; committed recovery removes journal/staging metadata without reverting installed files.
- On prepared recovery, treat each changed destination as one of: unchanged preimage (no action), exact expected postimage (safe to restore), journal-authorized read-only transition state, or conflict (preserve and report). Revalidate every path immediately before destructive recovery operations.
- Keep a prepared journal when any conflict or restoration error remains so a later recovery attempt can continue idempotently.
- Leave newly created empty parent directories in place during rollback rather than risking removal of a user-created directory; document this harmless residue.
- Reject a new apply while a pending journal exists before planning, and direct the user to `--recover`; repeat the check immediately before the transaction to cover a journal appearing after planning.
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

This design does not make an `adopt --apply` visible as one atomic filesystem change. After process interruption, rerun the explicit recovery command. If any managed output differs from both recorded states, or a Windows writable variant is not authorized by the journal transition state, the command reports it and preserves the user's bytes and permissions; manual resolution remains necessary. A crash between clearing the bit and recording `attributes_cleared` therefore requires manual permission restoration before recovery can continue.

The journal checksum detects uncoordinated edits, not a malicious same-user actor who can rewrite both the journal and its checksum. Recovery trusts the target worktree's Git metadata; it does not promise safety against hostile concurrent filesystem mutation.

## Source

- [Toe-Tickling Issue #28](https://github.com/Cucklord420/Toe-Tickling/issues/28)
- [Rust `std::fs::rename` platform behavior](https://doc.rust-lang.org/std/fs/fn.rename.html)
- `src/adopt.rs` at `main` commit `d2d7c837a7d5e62ce1c1d6ee8f8a61b2edda8824`
- `src/guide.rs` at the same commit, for in-process rollback and conflict-preservation patterns
