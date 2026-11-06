# Rust CLI Root Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore the old CLI behavior of selecting its own repository when `build` or `check` runs from an unrelated working directory.

**Architecture:** Keep argument parsing and `--root` unchanged. When no override is present, use Cargo's compile-time manifest directory as the selected checkout. A Rust integration test will launch the actual binary from a temporary unrelated directory and exercise both commands.

**Tech Stack:** Rust 1.99.1, edition 2024, Cargo integration tests, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-10-30-cli-parity-design.md`

## Global Constraints

- All new executable code and tests are Rust.
- The original Python implementation is reference material only; do not reintroduce it.
- An explicit `--root PATH` continues to override the default.
- Preserve the current Rust policy expansion and document its intentional output differences.
- Validate the exact final commit; do not claim local Windows compilation while `link.exe` is unavailable.

## Review Focus

- Unrelated current directory: the real `build` and `check` commands must select the crate root and succeed.
- Explicit override: `--root PATH` must continue to select the requested checkout.
- Paths with spaces: the existing real CLI adoption test remains green.
- Generated files: running `build` from an unrelated directory must not create guidebook files there.
- Invalid target root: an explicitly selected invalid root must continue to fail closed.

---

### Task 1: Pin real CLI root behavior

**Files:**
- Modify: `tests/validation.rs`
- Test: the new integration test `build_and_check_default_to_manifest_root_from_unrelated_directory`

**Interfaces:**
- Uses `CARGO_BIN_EXE_toe-tickling` to launch the actual Rust CLI.
- Runs from a fresh temporary directory that does not contain `policy.json`.

- [x] **Step 1: Add the regression test.** Added the real-CLI regression for `build`, `check`, and adoption preview from an unrelated temporary directory; it also asserts no launch-directory writes.
- [ ] **Step 2: Verify RED against the old default.** Not observed locally: the focused command stopped before test execution because MSVC `link.exe` is unavailable. The regression is designed to fail with the prior current-directory default; exact-head CI will establish the final behavior.
- [x] **Step 3: Implement the minimal root fallback.** `src/main.rs` uses `PathBuf::from(env!("CARGO_MANIFEST_DIR"))` when `--root` is absent and preserves explicit parsing. Help text now explains the default and override.
- [x] **Step 4: Verify GREEN at the source-fix head.** Initial run #42168654175 passed Windows, formatting, and Clippy, but Linux validation tests had 5 failures: four temporary fixtures omitted the newly linked parity matrix, and one CLI test implicitly used its current directory as the checkout. The fixtures now include the matrix, and the CLI test passes `--root` explicitly. Exact-head run [#42169337426](https://github.com/Cucklord420/Toe-Tickling/actions/runs/42169337426) passed Linux and Windows at `3ae0c8755c14d42d8c27f3232b51c657ebe35acd`. This documentation evidence update requires a fresh final-head run. The local focused test cannot link because `link.exe` is unavailable.

### Task 2: Record migration evidence and boundaries

**Files:**
- Create: `docs/PYTHON_RUST_PARITY.md`
- Modify: `docs/VALIDATION.md`
- Modify: `docs/RUST_MIGRATION.md`
- Modify: `tests/validation.rs` only if the repository fixture must copy the new public document for link/security checks.

- [x] **Step 1: Map every pinned Python test scenario to the current Rust test or to an explicit deviation.** `docs/PYTHON_RUST_PARITY.md` maps all 45 cases to current Rust coverage or a named gap. The pinned source/test/generated-guide hashes match the archived Linux run. A fresh rerun is unavailable because this environment has no Python interpreter. Four Windows skips are source-derived and explicitly not claimed as an observed Windows run.
- [x] **Step 2: Classify differences.** The parity matrix records expanded generated output, JSON-versus-human-readable `check` output, and Rust-only added commands as intentional, owner-approved differences rather than exact parity.
- [x] **Step 3: State remaining limits.** The matrix distinguishes historical Python evidence from current Rust execution and identifies the missing full differential fixtures, local linker, and macOS coverage.

### Task 3: Validate and prepare the canonical change

**Files:**
- Verify: full working tree and exact branch head.

- [x] **Step 1: Run formatting:** `cargo fmt --all -- --check` passed with Rust 1.99.1.
- [ ] **Step 2: Run lint:** `cargo clippy --workspace --locked --all-targets -- -D warnings` was attempted but stopped before project compilation because `link.exe` is unavailable.
- [ ] **Step 3: Run tests:** The focused regression was attempted and stopped during dependency build for the same missing linker; no tests executed locally.
- [ ] **Step 4: Run repository checks:** Local `cargo run` checks are blocked by the same linker; exact-head CI remains required.
- [ ] **Step 5: Require exact-head Linux and Windows CI; keep Issue #23 open until its acceptance evidence and default-branch verification are complete.
