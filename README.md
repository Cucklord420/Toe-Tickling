# Justin's AI Guidebook

The public constitution for work performed for Justin: one source of standing rules for every model, repository, project, research task, and automation.

**Models start with [GUIDEBOOK.md](GUIDEBOOK.md).** It is a complete, standalone document containing the policy and deliberately limited public profile. Read it before doing work, then apply the current request and relevant project instructions. The source modules are under `rules/` and `profile/PUBLIC.md`; the single-file version is generated and checked for drift.

## Point any model here

> Read https://github.com/Cucklord420/Toe-Tickling/blob/main/GUIDEBOOK.md in full and use it as my standing guidebook for this task and future work where your platform can retain it. Apply my current instructions and the project's specific requirements. Keep private details local and ask for them only if essential. Do not claim you loaded, remembered, tested, or completed anything unless you actually did.

Raw entry point: https://raw.githubusercontent.com/Cucklord420/Toe-Tickling/main/GUIDEBOOK.md

For a model without browsing, attach or paste `GUIDEBOOK.md`. [The reusable prompt](prompts/USE_WITH_ANY_MODEL.md) and [standing-memory text](prompts/STANDING_MEMORY.md) are also included. A public URL alone cannot change another model's memory or permissions.

## What's included

- Direct communication, source verification, current availability, arithmetic, TCO, and complete task delivery.
- Rust-first engineering, copyleft, real launcher/package testing, honest CI evidence, safe migrations, and data protection.
- A mandatory LLM execution contract, detailed task playbooks, and instruction audits at task start, before final delivery, and after material policy/tool changes or failures.
- Privacy boundaries, autonomous work within authorization, durable context, real automations, and limited day-to-day preferences.
- Entry points for AGENTS.md readers, Claude Code, Copilot, Gemini CLI, and Cursor; safe installation that preserves existing project instructions.
- A dependency-free builder/validator, regression tests, and least-privileged CI. These checks verify repository integrity; they cannot prove a model obeys prose.

## Use it in a project

From a checkout of this guidebook, preview the additions:

```sh
python3 scripts/guidebook.py adopt /absolute/path/to/project
```

Apply the same reviewed additions:

```sh
python3 scripts/guidebook.py adopt /absolute/path/to/project --apply
```

The tool adds a managed section to `AGENTS.md`, a local guidebook snapshot and digest lock, and private-file ignore rules. It creates model adapters only when absent, preserving existing ones and reporting them for manual integration. It does not commit, push, change account settings, fetch code, or run a model. See [adoption and platform setup](docs/ADOPTION.md).

## Maintain and verify

Python 3.10 or later and Git are sufficient; no paid services or third-party Python packages are required.

```sh
python3 scripts/guidebook.py build
python3 scripts/guidebook.py check
python3 -m unittest discover -s tests -v
```

Edit the source modules, update `policy.json`/`CHANGELOG.md` when policy changes, and rebuild. Generated artifacts must remain byte-identical to the sources. [CONTRIBUTING.md](CONTRIBUTING.md) explains the change process; [provenance](docs/PROVENANCE.md), [decisions](docs/DECISIONS.md), [validation](docs/VALIDATION.md), [the instruction audit](docs/INSTRUCTION_AUDIT.md), and [sources](docs/SOURCES.md) explain what the repository does and what has actually been checked.

The full guide is deliberately detailed for LLMs. The short adapters are loading instructions, not substitutes for the complete policy. The audit covers internal contradictions, missing requirements, stale technology assumptions, false certainty, privacy/authority boundaries, real entry points, and evidence quality. It requires reviewable findings and rechecks after fixes; self-certification alone cannot establish compliance.

## Public/private boundary and cost

The public profile includes Justin's name, general NYC area, timezone, and non-sensitive preferences. It deliberately excludes private records and precise personal details. Justin supplies private task context locally at his discretion; the blank [private template](profile/PRIVATE.example.md) is optional.

**TCO: $0 required software/subscription cost over one or three years** for this repository and its dependency-free local checks. Standard GitHub-hosted Actions runners are free for public repositories under GitHub's current terms; this workflow uses one standard Linux runner and no paid service or stored build artifact. Optional model/API use, local hardware/electricity, and future hosting choices are separate and usage-dependent. This is not a promise that every future project has zero cost.

Sources: GitHub — 10-15-2026 — https://docs.github.com/en/actions/concepts/billing-and-usage. Date is a review date for this undated page.

## License

[AGPL-3.0-only](LICENSE). Copyright 2026 Justin (GitHub: Cucklord420). This guidebook's license covers its contents and tools; adopting its preferences does not relicense an existing project or expand GPL obligations to unrelated work. Preserve inherited licenses. See [NOTICE.md](NOTICE.md).
