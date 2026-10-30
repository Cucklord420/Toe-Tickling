---
name: self-checks
description: Apply rigorous, evidence-backed self-checks when creating, editing, reviewing or shipping code, agent skills, CI, documentation, or deliverables. Use real tests, negative fixtures, exact-commit verification and risk-based acceptance instead of repeatedly asking an AI whether its own work looks correct.
license: AGPL-3.0-only
---

# Evidence-backed self-checks

Use for code, repositories, tools, agent skills, tests, docs, migrations and releases. This is a *workflow* and must never substitute for executable tests, target-platform execution, review, or the current repository's own acceptance rules.

## Select the real target

1. Read existing repository policy, `AGENTS.md`, source, CI, tests, and issue/PR context. Record the exact branch, SHA, tools, risk, real entry point, observable expected behavior and existing failures. Preserve user data, privacy, permissions, history and concurrent work.
2. Before changing a file, define success criteria that can **fail** for incorrect behavior. Reuse established test suites, regression fixtures, verified source examples, and direct user requirements. Do not invent assertions merely to enlarge a number.
3. Start with no-write inspection and a baseline. Do not run scripts whose side effects are uncertain. Keep destructive validation in owned, isolated, disposable fixtures; do not deploy or operate on live customer/production resources.

## Build meaningful coverage

Prefer independent evidence from the **actual code path** and application behavior over verbal AI self-evaluation. Check relevant perspectives, normally including:

- Compilation, formatting, strict linting, dependencies and license safety.
- Happy paths, invalid inputs, boundary values, empty inputs, malformed state and error messages.
- Permissions, secrets, injection, untrusted sources, identity and path traversal.
- Data preservation, rollback, error cleanup, interrupted operations, and restart behavior.
- Concurrency, races, idempotence, cancellation and timeouts.
- Cross-platform behavior, compatibility, version drift and feature/upgrade parity.
- Real public launcher, package contents, missing assets, docs references and final outputs.
- Observable integration behavior, external service constraints and exact user-facing acceptance.
- Unsupported or skipped checks, flaky tests, false-positive passes, and deliberately broken fixtures.
- TCO, energy or maintenance where material to an engineering decision.

Unit tests prove bounded logic. Integration tests prove components together. A release requires launch and artifact tests. No single layer proves another. Rust's ownership and type checks are strong safeguards, not proof of functional correctness or data safety.

## Self-checks

1. Run an applicable **baseline** before editing. Determine what already passed, failed, was unavailable, or had side effects.
2. Create at least one targeted **negative** fixture for each material new contract: demonstrate that the check fails on a deliberately broken case before trusting a green result. Preserve failing and passing expectations.
3. Run relevant formatting, compiler, lint, unit, integration, packaging and platform checks against the actual revision. For a large change, map at least ten genuinely different applicable failure perspectives, not ten repetitions of one test.
4. Test the **real entry point**, not only internal helpers or a mock. Verify correct exit status, user-visible output, generated files, data integrity and compatibility.
5. Recompute calculations, reconcile source facts, review the full diff, examine privacy and permission boundaries, and actively look for a counterexample.
6. Refresh branch/base/CI/review state before writing or merging. Old CI is invalidated by new commits. Never bypass branch protections, reviews, authorization, or independent approval.
7. After each material fix, rerun affected tests and fresh CI on the exact final SHA. Record command, environment, counts, skipped tests and outcome, with direct GitHub URLs.
8. At handoff distinguish **implemented**, **tested**, **merged**, **released** and **deployed**. Report every unverified requirement or blocker without substituting an invented pass.

## Failure handling

Stop the affected action when a required gate fails, source/target identity changes, a destructive operation is uncertain, permissions are missing, or evidence is absent. Diagnose once from concrete logs, change the approach, and retest; do not rerun the same deterministic failure indefinitely. Continue safe independent work. Never weaken a check to get a green badge, self-approve, overwrite a competing branch, invent source data, or claim that the checklists prove model compliance.

## Acceptance cases

The companion `references/self-check-cases.json` defines separate positive and negative review cases with expected and forbidden outcomes. They are **scenario specifications**, not automatically executed model tests. The repository's Rust skill validator checks their structure and rejects missing or malformed cases. Actual model behavior and external integrations require independently run evaluations with recorded outcomes.
