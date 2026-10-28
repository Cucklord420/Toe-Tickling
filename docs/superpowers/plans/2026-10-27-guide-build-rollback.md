# Guidebook build rollback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure a returned failure during generated-file persistence restores the prior coherent output set or retains explicit recovery backups.

**Architecture:** Keep validation/rendering and current output paths unchanged. Add a private staged-output transaction in `src/guide.rs` that stores new files and adjacent backups, commits each file atomically, and rolls completed replacements back in reverse order if a later operation fails.

**Tech Stack:** Rust 1.99.1, Cargo workspace, `tempfile`, standard filesystem APIs; no new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-27-guide-build-rollback-design.md`

## Global Constraints

- All new executable code and tests must be Rust.
- Keep `build` output paths, bytes, return value, and `check` behavior compatible.
- Validate and stage all outputs before the first destination replacement.
- Revalidate destination ancestors through the root before each replacement and rollback; reject symlinks introduced after staging.
- Preserve existing file permissions when backing up and restoring outputs.
- Reapply original permissions after Windows tempfile retention and restoration finalization.
- Never overwrite a destination changed by another writer during rollback; retain the backup and report its path.
- Do not claim atomicity across a process crash or power loss.

## Review Focus

- A destination that was originally absent must be removed if a later commit step fails.
- A destination that existed must be restored byte-for-byte and retain its permissions.
- If rollback sees content other than this transaction's new bytes or the original bytes, it must preserve the concurrent change and keep the backup.
- If an ancestor becomes a symlink after staging, the next replacement and rollback must reject the path and avoid following it; retain and report the prior output's backup.
- Retained Windows backups and restored destinations must preserve the original readonly attribute.
- A genuine later persistence failure with an unchanged destination must roll back prior replacements and clean staged files.
- Validation/path-collision errors must still happen before any generated file changes.
- Successful builds must leave no transaction files behind and retain the existing output-count behavior.

---

### Task 1: Prove rollback after a later output failure

**Files:**
- Modify: `src/guide.rs`
- Existing integration coverage: `tests/validation.rs`

**Interfaces:**
- `commit_outputs` accepts staged output records and an internal `FnMut(&str, usize) -> Result<()>` hook called after each successful replacement; production passes an always-successful hook, while unit tests inject a failure after the second replacement.
- Each staged record carries the relative output name, destination path, new bytes/staged file, original bytes and permissions, optional backup, and whether replacement completed.

- [x] Add a unit regression with two existing files and one originally absent file; inject a failure after the second successful replacement and assert the files are restored exactly, original Unix permission bits return, and the absent file remains absent.
- [x] Add regressions for concurrent changes after staging and during rollback; assert changes are preserved and the original backup path is reported and retained.
- [x] Run the exact-head hosted Ubuntu regression job and confirm the pre-implementation failures: overwritten original output, accepted post-staging change, and missing retained backup.
- [x] Implement backup staging and reverse rollback. Preserve backup files and include their paths in the returned error if rollback cannot safely restore.
- [x] Run the focused test and existing guide-build validation regressions.
- [x] Fresh review caught path-ancestor revalidation and Windows readonly metadata gaps; add red regressions before fixing them.
- [x] Exact-head test checkpoint 1e45af065577ff3238450a3bb6ce47e87ef96544: Ubuntu symlink-swap regression failed as expected; Windows readonly-backup and readonly-restoration regressions failed as expected; the Windows locked-target real-persist-failure regression passed.
- [ ] Revalidate paths before replacement/removal/restoration and reapply original permissions after `keep`/`persist` finalization.
- [ ] Run exact-head Ubuntu and Windows checks after the fix.

### Task 2: Route `build` through the transaction

**Files:**
- Modify: `src/guide.rs`
- Existing integration coverage: `tests/validation.rs`

- [x] Keep policy validation, rendering, output-path preflight, and complete new-output staging before commit.
- [x] Verify a validation failure leaves all generated outputs byte-for-byte unchanged.
- [x] Replace the sequential persist loop with `commit_outputs`; retain the number-of-outputs return value.
- [x] Add assertions for permission preservation and backup retention when rollback detects a concurrent destination change.
- [x] Run `cargo fmt --all -- --check`, `cargo clippy --workspace --locked --all-targets -- -D warnings`, `cargo test --workspace --locked`, `cargo run --locked -- check`, and `cargo run --locked -p guidebook-audit -- check .` on exact head in Ubuntu Actions run 40606300518.
- [ ] Inspect the complete diff and confirm no generated policy files or unrelated adoption changes are included.

### Task 3: Verify exact branch state

**Files:**
- No further source changes unless a validation defect is found.

- [x] Confirm the branch remains based on `d50fe7c8c0a94bb32676b5042f53adfead0e7ca0`; GitHub `main` still points there.
- [x] Obtain exact-head Ubuntu workflow results before opening a canonical PR for existing Issue #35: run 40606300518 passed all required jobs at 6874af644decc67859ef6dfe6f91d3f6fe21c345.
- [ ] Complete independent branch review after the review-driven fix, inspect the complete diff, and open one canonical PR only after exact-head validation.
- [ ] Keep Issue #35 open until the fix is merged and verified on `main`.
