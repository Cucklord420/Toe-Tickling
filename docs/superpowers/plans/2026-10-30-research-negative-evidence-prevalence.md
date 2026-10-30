# Research negative evidence and prevalence implementation plan

## Goal

Address Toe-Tickling Issue #44 with a distinct stable Rust-validated rule and two focused semantic scenarios, without changing prior IDs or implying a required review-site sweep.

## Architecture

Canonical policy remains in Markdown under `rules/`; `src/guide.rs` enforces the stable standing-rule family; `docs/SCENARIOS.md` holds semantic acceptance cases. Rust generates GUIDEBOOK, adapters, and integrity artifacts.

## Tech Stack

Rust 1.99.1 toolchain pinned by the repository; Cargo locked workspace commands; Markdown and JSON for declarative policy and generated artifacts.

## Spec

Implement Issue #44: seek relevant credible negative evidence; separate independent corroborated reports from copied, anecdotal, stale, and resolved items; provide time windows and denominators only when supported; do not claim incidence from unrepresentative counts; revisit recommendations when unresolved material downsides matter. Keep all source work in the existing isolated branch.

## Global Constraints

- Preserve RES-001 through RES-012 and all verification IDs/history.
- Do not publish private transcripts or personal evidence.
- Do not claim semantic adoption from mechanical tests.
- Do not claim any test or CI passed unless its result is observed.
- Do not merge; leave issue and PR state open pending exact-head validation and review.

## Review Focus

Check stable-ID preservation, generated source/digest/manifest consistency, accurate count and version updates, the absence of a site-by-site mandate, and whether examples avoid fabricated prevalence rates.

## Tasks

1. Add a regression in `tests/validation.rs` before implementation that fails when a declared `RES-013` is removed; attempt the targeted run and record toolchain limitations accurately.
2. Add `RES-013` in `rules/20-research.md`, add duplicate-report and denominator-free severe-report scenarios, and raise the Rust stable family gate in `src/guide.rs`.
3. Update `policy.json` to 1.2.5 / 10-30-2026 and add the Issue #44 rationale to `CHANGELOG.md`.
4. Run `cargo fmt`, locked Clippy/tests, `cargo run --locked -- build`, deliberately refresh the reading manifest, and inspect every generated diff.
5. Run the repository's full local validation, inspect the final diff and branch state, then push and open one draft canonical PR for Issue #44 if the result is reviewable. Keep the issue open until reviewers and required checks verify completion.
