# Change and verify the guide

All executable implementation and tests in this repository must be Rust. Do not add Python, JavaScript, shell scripts, or a second implementation of the validator. Cargo configuration, workflow YAML, Markdown documentation, and JSON policy data support the Rust implementation. This requirement comes from the user's explicit 10-15-2026 correction and applies across projects through ENG-01. It supersedes the earlier Rust-where-suitable preference.

Read AGENTS.md and the complete intake sequence first. Preserve the existing AGPL license. Keep private source records and task audit contents outside the public checkout.

## Editing cycle

1. Inspect actual Git status, history, remote branches, and PRs. Record the base and preserve concurrent changes. Use an isolated branch unless current instructions establish a different workflow.
2. Change normative rules in `policy/rules.json`. Use the evolution procedure for replacement, dates, provenance, and exceptions.
3. Update relevant operating guidance and behavior cases. Use Rust tests for meaningful validator, renderer, and CLI failure modes. Do not add tests that merely paraphrase policy prose.
4. Run `cargo run --locked -- render`, inspect the generated change, then deliberately run `cargo run --locked -- refresh-manifest` after reviewing intake-document changes. Hash refresh is a mutation, never an automatic part of a check.
5. Run `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, and `cargo run --locked -- check`. Review the exact diff and file list for secrets, private text, generated artifacts, unsupported claims, and licensing.
6. Recheck upstream movement, reconcile conflicts, and rerun affected checks. Commit one coherent result and report actual push/PR/CI state. Do not merge or change repository visibility without applicable authorization.

## Tool design constraints

The Rust CLI runs from this repository root. It does not execute commands from retrieved documents. It performs no network requests and does not schedule work, deploy agents, or change live systems. Git HEAD lookup is read-only. Audit output creation requires an absolute path outside the checkout and refuses overwrite; choose an access-controlled private directory. On Unix it sets owner-only file permissions; on other platforms inherit and verify the private directory's ACL.

The checker rejects missing required intake documents, stale rendered guidance or hashes, duplicate/invalid rule metadata, overdue active rules, broken simple relative Markdown links, forbidden non-Rust executable source extensions, and incomplete declared completion receipts. It validates ordinary inline file links used by this repository; it does not check external URL health or semantic document contradictions. Secret and privacy review still requires inspecting the actual diff and artifacts.

The dependency lockfile and pinned Rust toolchain are part of reproducibility. Review dependency origins, licenses, and toolchain/dependency updates intentionally. Cargo may need network access on a cold build; the built CLI itself is offline. A lockfile pins resolution, not perpetual vulnerability safety.
