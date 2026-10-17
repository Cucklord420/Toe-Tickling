# Primary technical references

Reviewed 10-15-2026. The dates below are review dates for undated live documentation, not invented publication dates. Tool behavior can change; recheck official documentation before changing an integration. These references support compatibility/security/cost statements, not claims that every model always complies.

- Sources: AGENTS.md — 10-15-2026 — https://agents.md/. Review date; predictable agent instruction file and repository/nested scope.
- Sources: GitHub — 10-15-2026 — https://docs.github.com/en/copilot/how-tos/copilot-on-github/customize-copilot/add-custom-instructions/add-repository-instructions. Review date; Copilot's repository and agent instruction files.
- Sources: Anthropic — 10-15-2026 — https://code.claude.com/docs/en/memory. Review date; CLAUDE.md, explicit imports, and memory limitations.
- Sources: OpenCode — 10-15-2026 — https://opencode.ai/docs/rules/. Review date; AGENTS.md, global rules, and instructions configuration. Markdown links do not automatically inline a referenced file.
- Sources: Google — 10-15-2026 — https://google-gemini.github.io/gemini-cli/docs/cli/gemini-md.html. Review date; GEMINI.md context and imports.
- Sources: Cursor — 10-15-2026 — https://cursor.com/docs/rules. Review date; AGENTS.md and always-applied `.mdc` rules. A file mention is not automatic content inclusion.
- Sources: GitHub — 10-15-2026 — https://docs.github.com/en/actions/reference/security/secure-use. Review date; minimum permissions, untrusted contributions, and immutable action pins.
- Sources: GitHub — 10-15-2026 — https://docs.github.com/en/actions/concepts/billing-and-usage. Review date; free standard hosted runners for public repositories, usage limits, and separate private-repository quotas.

## Official Rust engineering sources

Reviewed 10-17-2026. These are review dates, not publication dates. Consult the sections relevant to the code under review, the actual pinned toolchain and the original program's behavior. These references do **not** establish that a port was built, tested, installed, or behaviorally equivalent.

- Sources: Rust Project — 10-17-2026 — https://doc.rust-lang.org/stable/book/. Review date; task-relevant ownership, error-handling, testing, and concurrency chapters before Rust implementation.
- Sources: Rust Project — 10-17-2026 — https://doc.rust-lang.org/reference/. Review date; specific language semantics where correctness or compatibility depends on them.
- Sources: Rust Project — 10-17-2026 — https://doc.rust-lang.org/std/. Review date; exact standard-library APIs and platform-specific guarantees.
- Sources: Rust Project — 10-17-2026 — https://doc.rust-lang.org/cargo/guide/continuous-integration.html. Review date; Cargo CI setup and version-specific testing; a workflow file is not a successful run.
- Sources: Rust Project — 10-17-2026 — https://doc.rust-lang.org/cargo/reference/rust-version.html. Review date; the package `rust-version` (MSRV) declaration, dependency resolution, and explicit support/testing expectations.
- Sources: Rust Project — 10-17-2026 — https://doc.rust-lang.org/clippy/. Review date; Rust linting, with applicability checked against the pinned toolchain.
- Sources: Rust Project — 10-17-2026 — https://doc.rust-lang.org/edition-guide/. Review date; edition migration and compatibility, not an automatic mandate to change editions.
- Sources: Rust Project — 10-17-2026 — https://doc.rust-lang.org/releases.html. Review date; verify stable releases against the actual pinned compiler; never assume the newest APIs are available to the current project.
- Sources: Rust library team — 10-17-2026 — https://rust-lang.github.io/api-guidelines/. Review date; **advisory** library/API design checklist, not binding standards or grounds to change public APIs without parity testing.
- Sources: Rust Project — 10-17-2026 — https://doc.rust-lang.org/nomicon/. Review date; advanced `unsafe` and FFI invariants only when those facilities are present; the book notes its own incompleteness.

The complete GNU AGPL v3 text is preserved unmodified in `LICENSE`. Licensing statements describe this repository's choice and preserve upstream obligations; no claim is made that a license forces disclosure of unrelated programs or all private modifications.
