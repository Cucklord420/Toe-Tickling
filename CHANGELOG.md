# Changelog

## 1.2.2 — 10-16-2026

- Applied the owner's latest instruction: use the official Rust Book, Style Guide, Clippy, Cargo documentation, and task-relevant Reference pages instead of reading every page of the Rust Reference before each task.
- Retained pinned toolchain/edition checks, formatting, linting, actual-entrypoint/parity tests, explicit safety boundaries, and controlled performance evidence.
- Regenerated the guide, rule register view, digest lock and reading manifest; Rust CLI and CI validation remain required before merging.

## 1.2.1 — 10-15-2026

- Added mandatory complete official stable Rust Reference reading for every AI agent undertaking Rust development, review, porting, or optimization (ENG-020 and ENG-07), with chapter/page evidence instead of unverified reading claims.
- Required a fresh official stable-release/Reference check before each new Rust task, review of changed material after releases, and compatibility checks against the repository-pinned compiler and edition; documented evidence gates when access is incomplete.
- Made measured release-mode profiling and correctness/parity regression checks prerequisites for performance claims; clarified that the Reference defines language behavior rather than guaranteeing fast code.
- As of this review, the official release notes list stable Rust 1.99.1; this repository still pins Rust 1.99.0 and edition 2024. No toolchain upgrade was made.

## 0.1.0 — 10-15-2026

- Established a living rule register and generated readable guide with scoped requirements and observable self checks.
- Added intake, action, execution, completion, and policy-evolution audits; adoption guidance and semantic failure scenarios.
- Made Rust mandatory across projects for all newly authored executable code and tests, superseding the earlier suitability preference. Added Rust-only tooling and regression tests. Documentation and policy data remain Markdown and JSON.
- Added source coverage and privacy boundaries. Initial review inventoried 13 repositories; selected sources were inspected, not the entire historical corpus.
- Preserved the existing license and isolated changes from other workers. Model adoption, production deployment, and continuous monitoring are separate unperformed work.

## 1.2.0 integration — 10-15-2026

- Reviewed and integrated the other agent's published `0bdc6fe` guidebook: retained the existing three-digit rule IDs, playbooks, public profile, model adapters, source references, and historical audit records.
- Replaced the Python maintenance/adoption implementation and Python tests with Rust, removing the maintenance-language exception and strengthening both ENG-001 and ENG-01 to the current mandatory Rust instruction.
- Preserved later EVO-001 through EVO-028 and ENG-015 through ENG-019, bringing the module policy to 158 stable IDs. Added 45 scoped verification rules, reading-hash receipts, policy freshness checks, negative regression coverage, and a combined generated GUIDEBOOK.md. Historical Python validation results remain labeled historical and do not validate this revision.
