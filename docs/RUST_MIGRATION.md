# Rust implementation and migration evidence

All current executable code and tests are Rust, under the user's explicit mandatory-language instruction. The former Python builder/adopter and tests were removed from the active tree; their source and measured results remain in Git history and [the historical phase report](archive/INITIAL_RUST_MIGRATION.md).

## Current components

- Root Cargo package: typed policy validation, deterministic full guide/register/adapters, hashes, private audit templates and completion gates, public-file/privacy/link/CI checks, and safe adoption into existing Git projects.
- Independent `rust/audit` workspace package: direct generated-module byte comparison and separate rule/source/adapter checks. It complements the primary checker; it does not prove semantic truth or live model behavior.
- Rust integration tests: synthetic Git fixtures for actual CLI use, unsafe inputs and targets, tampering, ignored versus tracked files, repeat installation, source updates, and stale-plan rejection.

## Verification

```bash
cargo test --workspace --locked
cargo run --locked -- check
cargo run --locked -p guidebook-audit -- check .
```

See [current validation](VALIDATION.md) for measured results. Policy and output differences are intentional: mandatory Rust supersedes the old language exception, new standing rules and verification procedures expand the full guide, and versioned locks cover the new source set. Original behavior requirements were reviewed directly; the Python implementation was not executed for this migration.

Linux behavior is measured with disposable Git projects. The historical Python-to-Rust coverage review and intentional output differences are recorded in [the parity matrix](PYTHON_RUST_PARITY.md). Issue #23 adds real-CLI coverage for invoking `build`, `check`, and adoption preview from an unrelated current directory; its exact-head Linux and Windows checks must be read from the canonical PR. This focused regression does not establish full cross-language differential parity.

The current Windows authoring environment has Rust 1.99.1 installed but lacks the MSVC `link.exe`, so local executable tests cannot link. It also has no usable Python interpreter, preventing a fresh rerun of the pinned historical Python suite. The archive records a 45-test Python 3.12.14 Linux pass from 10-15-2026; that historical result is not a current cross-platform or exact-head run. Windows/macOS native execution, real provider loading, global account settings, and cross-session memory remain unverified. Writes are atomic per file, not a multi-file transaction; hostile concurrent filesystem mutation remains a limit. No deployment or production change is implied by a source migration.
