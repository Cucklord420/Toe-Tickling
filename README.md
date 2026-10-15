# Toe-Tickling: the living rule book

A practical operating law for the LLMs and agents I use: inspect the actual situation, follow my applicable instructions, do the work, check yourself, and report the truth. Nothing in life stays static. Neither does this guide.

The guide must evolve with technology, evidence, and my decisions. Strong rules need clear scope, observable checks, and a correction process. Calling the guide gospel does not make unsupported claims true or give an agent permissions it does not have.

## Read and apply

Begin with [AGENTS.md](AGENTS.md). Read the [rule book](RULEBOOK.md) and its linked operating procedures. The compact retrieval entry point is [llms.txt](llms.txt). Rules have stable IDs, scope, provenance classes, review dates, and checks in [the canonical register](policy/rules.json).

- [Self checks](docs/SELF_CHECKS.md): intake, execution, recovery, final review, and evidence receipts.
- [Evolution](docs/EVOLUTION.md): changing requirements, conflict resolution, review cadence, and concurrent agents.
- [Adoption](docs/ADOPTION.md): configuring different agents and proving behavior.
- [Scenarios](docs/SCENARIOS.md): adversarial and failure-path acceptance cases.
- [Provenance](docs/PROVENANCE.md): what was inspected and what remains unknown.
- [Contributing](CONTRIBUTING.md): maintainers' change and verification procedure.
- [Changelog](CHANGELOG.md): versioned changes and scope limits.

## Check locally

Rust toolchain pinned in `rust-toolchain.toml`; locked Cargo dependencies:

```bash
cargo run --locked -- render --check
cargo run --locked -- check
cargo test --locked
cargo run --locked -- new-audit --task "Describe the task" --output /absolute/private/path/audit.json
cargo run --locked -- check --audit /absolute/private/path/audit.json
```

The last command intentionally fails until the record has real evidence and resolved checks. Keep real audit records private. The checker validates required documents, render consistency, rule metadata, local links, review deadlines, and declared receipts. It cannot prove that a model read honestly, that a citation is true, or that a live system works.

## Current delivery boundary

The initial review inventoried all 13 accessible repositories and inspected selected instructions, project records, and historical conversations. It did not read every historical message or private release asset. At the initial snapshot, this repository contained only its README and license; no other agent's published branch or pull request was visible. See provenance for exact coverage limits and follow-up work. No model deployment or cross-repository rollout is implied.

## License

Original additions follow the existing [AGPL-3.0-only license](LICENSE). See [license scope](LICENSE-SCOPE.md). Private source material remains private and is not republished or relicensed by this guide.
