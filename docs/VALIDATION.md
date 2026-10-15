# Current Rust validation

This report describes policy 1.2.0. Historical Python results apply only to the earlier source revision and are preserved in [the original report](archive/INITIAL_VALIDATION.md).

The Rust toolchain is pinned in rust-toolchain.toml and dependency resolution is pinned in Cargo.lock. Current validation commands are:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
cargo run --locked -- check
cargo run --locked -p guidebook-audit -- check .
```

Measured on 10-15-2026, Linux x86_64, Rust 1.99.0: formatting passed, workspace Clippy passed with warnings denied, all 83 Rust tests passed with no failures or skips, and both real audit CLIs passed. The primary checker verified 158 standing rules plus 45 verification rules. The independent checker verified 14 ordered modules, exact source-byte rendering, source/guide digests, strict metadata, rule declarations, register identity, and adapters.

The 83 tests comprise 24 safe-adoption fixtures, 26 public-tree/security/CI fixtures, 22 primary policy/receipt/CLI regressions, and 11 independent-audit fixtures. They are distinct tests, not 83 independent policy audits. Actual child-process CLI coverage includes a different working directory, a project path with spaces, read-only preview, successful installation, repeat idempotency, and no automatic commit. The independent checker also ran successfully from an unrelated directory. The `alias/.` negative case initially failed and now passes as a regression after the lexical-prefix fix.

A configured workflow is not a remote CI pass; inspect the actual published head's Actions result. No Windows/macOS native execution, hardware/storage destructive test, live-model provider compliance, account-wide adoption, or durable provider memory write is claimed. All adoption fixtures are disposable Git projects with fictional data. Privacy patterns and content hashes are partial structural safeguards and require review of the actual public diff.

The original AGPL license remains unchanged. The source modules' existing IDs and the complete combined bundle are validated by the Rust builder. Generated artifact identities belong to the actual checked revision; they do not inherit the initial Python suite's pass count.
