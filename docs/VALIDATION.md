# Rust validation

## Policy 1.2.3 — local evidence, 10-16-2026

Base: `4f519c3ea1ed120d36aefea014fbf31bc0593304`; review branch `audit/preserve-verification-history`. Linux x86_64, Rust 1.99.1, Cargo edition 2024. The official release notes retrieved on this date also identify 1.99.1 as stable; no compiler or dependency update was made. Consulted the official Rust Book's testing guidance and relevant standard-library/Cargo/Clippy/rustfmt/edition documentation for the focused validator change; no exhaustive Reference read or performance claim is made.

The new removal regression failed before the repair with `silently deleted AUTH-01`. After repair, `cargo test --workspace --locked` passed **87 tests**: 12 independent-auditor, 24 adoption, 26 security, and 25 validation tests, with no failures or ignored cases. `cargo fmt --all -- --check` and strict workspace Clippy also passed. After regeneration and manifest refresh, `cargo run --locked -- check` and `cargo run --locked -p guidebook-audit -- check .` passed with 159 standing and 46 verification rules. Remote CI evidence for the delivered commit belongs in its PR; the earlier workflow below validates only its explicitly identified tree.

The removal checks protect stable-ID retention and explicit evolution. They do not establish semantic truth, full historical source review, live model behavior, native Windows/macOS behavior, or deployment. The earlier migration and operational limits still apply.

## Policy 1.2.2 — prior GitHub Actions evidence

This prior report records observed checks on 10-16-2026, plus explicitly historical evidence. At its tested revision the canonical standing policy was version **1.2.2** with **159 standing rules** and **46 active verification rules**. Earlier Python results remain historical; see [the initial validation report](archive/INITIAL_VALIDATION.md).

## Exact GitHub Actions evidence

- **Tested PR:** [#14](https://github.com/Cucklord420/Toe-Tickling/pull/14), head `80f66b98e0f3193e49ce58951d8d417d49275e29`.
- **Successful workflow:** [Rule book audit run 38917082012](https://github.com/Cucklord420/Toe-Tickling/actions/runs/38917082012), on the pull-request merge commit `db1e54f342fe9987f6bef83c0640e131cb8ccda8`.
- **Merged main commit:** `ef959c387bcd4700bb9c2311adaed44823559c78`. The tested synthetic merge commit and the final squash-merged commit both have the **identical Git tree** `ead086107b4cd423ac8b7fea2c2d371c9cbf394f`; this is a verified content-identity comparison, not a claim that the commit IDs are identical.
- **Runner:** GitHub-hosted Ubuntu Linux x86_64; pinned Rust **1.99.1**, as upgraded in merged [PR #10](https://github.com/Cucklord420/Toe-Tickling/pull/10).

The successful workflow executed:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
cargo run --locked -- check
cargo run --locked -p guidebook-audit -- check .
```

The CI logs show format and Clippy passed, with all **84 distinct Rust tests passing**: 11 independent-auditor unit tests, 24 adoption/integration tests, 26 security/CI tests, and 23 validation/policy/receipt tests. All had zero reported failures or ignored cases. The primary CLI reported 159 standing rules and 46 verification rules. The independent audit reported 14 modules and the same policy counts. These results prove the covered structural and executable checks for the tested tree, **not** actual compliance by AI models.

## Migration and operational limits

The Rust toolchain and Cargo.lock are pinned. The Rust application is the current maintenance/build/validation path. The original Python implementation's complete behavioral parity was **not** established by the latest workflow: the test suite covers selected functional requirements using synthetic fixtures, not a comprehensive side-by-side Python/Rust differential run on every original behavior.

There is no verified native Windows or macOS run, destructive hardware/storage run, true model-provider instruction-loading/compliance experiment, production deployment, account-global rule installation, or durable provider-memory write. No status or GitHub CI pass can prove a model read or applied instructions semantically. Local link/privacy pattern checks are limited safeguards, not guaranteed secret absence.

The original AGPL-3.0-only license remains intact. Generated SHA-256 digests identify exact content, not truth, permission, or origin authority. The source of each new compliance claim must remain the actual revision, dated observation, and corresponding logs.

## Older evidence

The 10-15-2026 policy 1.2.0 run with Rust 1.99.0 and 83 reported tests is superseded for current CI status, but preserved in repository history. Do not carry that test count forward to policy 1.2.2. Reverify this report when implementation, policy meaning, toolchain, or the checked tree changes.
