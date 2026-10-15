# Current Rust validation

This report describes policy 1.1.0. Historical Python results apply only to the earlier source revision and are preserved in [the original report](archive/INITIAL_VALIDATION.md).

The Rust toolchain is pinned in rust-toolchain.toml and dependency resolution is pinned in Cargo.lock. Current validation commands are:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked -- check
```

Final measured results will be recorded after integrated tooling is tested. A configured workflow is not a remote CI pass. No Windows/macOS native execution, hardware/storage destructive test, live-model provider compliance, account-wide adoption, or durable provider memory write is claimed. All adoption fixtures are disposable Git projects with fictional data. Privacy patterns and content hashes are partial structural safeguards and require review of the actual public diff.

The original AGPL license remains unchanged. The source modules' existing IDs and the complete combined bundle are validated by the Rust builder. Generated artifact identities belong to the actual checked revision; they do not inherit the initial Python suite's pass count.
