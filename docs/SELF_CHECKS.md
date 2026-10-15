# Self checks throughout the task

Checks happen while reading and working, not just at the end. Keep a task-specific private record. Never include hidden chain-of-thought: record concise decisions, actions, evidence, and limitations. Record one concrete observation per check; do not repeat “looks good” across rows.

## Gate 1: intake and reading

Before substantive work:

1. Capture the task, acceptance conditions, existing authorization, repository commit, working-tree state, relevant environment, and current date/timezone.
2. Inventory applicable entry points, nested instructions, source material, workers, and unavailable context. Counts must distinguish inventoried, fetched, actually read, sampled, skipped, and inaccessible. A search miss does not prove absence.
3. Read the AGENTS.md intake sequence in full. At the end of each document, record its SHA-256, at least one applicable requirement, a concrete consequence for the task, and any stale assumptions or conflicts. Read the complete rule register through its generated view; do not claim that hashing it constitutes reading it.
4. Assign every active rule a disposition. Use `pass` only for an applicable check with evidence; `fail` for a known violation; `blocked` for an unresolved or unexecuted check; `not-applicable` only with a task-specific reason. Never use not-applicable to hide lack of tools or evidence.
5. List task acceptance conditions separately from rule compliance. An agent may follow process correctly and still fail to achieve the user's objective.

If a document is too large, read bounded sections in sequence with a cursor and coverage ledger. On context loss, reload instructions and the checkpoint and verify referenced evidence still matches current inputs. Do not retain a past “pass” after a relevant document, branch, permission, or target changes.

## Gate 2: before a material side effect

Record the action and expected result. Verify scope and authorization, exact target and destination, freshness, privacy, concurrent changes, dependencies, reversibility, and abort criteria. Recheck destructive targets immediately before execution. Reuse valid session authorization; do not ask the user to repeat it.

A failed precondition blocks the affected action. Continue independent authorized work. Permission, identity, and policy failures require resolving the actual boundary; do not retry through another path to bypass it. If approval is required, identify the exact rule or tool rejection and present the concrete reviewable action.

## Gate 3: during execution

At material checkpoints:

- Compare expected versus observed outputs, exit statuses, side effects, and resource use. Verify downstream behavior instead of inferring it from process exit alone.
- Check invariants, data counts, integrity hashes, duplicate handling, and privacy boundaries. Reconcile partial outputs against inputs.
- Exercise relevant malformed input, timeout, cancellation, unavailable service, partial write, restart, and rollback cases in an appropriate test environment.
- On failure, preserve useful sanitized evidence, diagnose the cause, and invalidate dependent checks. Change strategy or mark a blocker after two attempts without new evidence.
- After another worker or upstream change, inspect the diff again and rerun affected checks. Do not interrupt a separate active worker merely to obtain a cleaner audit.

## Gate 4: final acceptance

Before claiming completion:

1. Map every acceptance condition to observed evidence and explicit limitations. Unrun runtime checks stay unrun even if all static checks pass.
2. Inspect the final diff and artifact contents, links, licenses, secrets, audience, and revision. Confirm unrelated work remains intact.
3. Verify actual push, PR, CI, deployment, or live status only for actions performed. Do not describe a saved prompt as an active schedule or uploaded data as tested retrieval.
4. Resolve every failing applicable check or clearly report the blocked outcome. Keep exceptions visible; only the authorized user or higher-priority instruction can grant a task-specific exception, and neither can turn a failed test into a passing one.
5. Update the private checkpoint, source coverage, unresolved items, and concise final report. Include what changed, what ran, important limits, and actual delivery state.

## Receipt format and tools

Generate a record outside the checkout using `cargo run --locked -- new-audit`. It contains document hashes and placeholder notes; these are **not proof of reading**. Fill notes after reading. It starts with every check and acceptance condition blocked. Add real commands, observations, artifacts, or source references to the `evidence` map. Use stable evidence IDs in checks and acceptance conditions. Avoid secrets and unnecessary personal details even in local records.

Required fields are schema version, task, outcome, UTC recording timestamp, repository revision, reading receipts, evidence, rule checks, and acceptance conditions. `outcome` is `in-progress`, `complete`, or `blocked`. All active rule IDs must appear exactly once. A complete receipt must have no unresolved rule checks and every acceptance condition must pass. Not-applicable needs a concrete reason. A blocked record preserves partial progress but does not pass the completion gate.

Run `cargo run --locked -- check --audit /absolute/private/path/audit.json`. Structural validity, complete declarations, and matching hashes do not prove truthful content, current runtime state, or correct semantic applicability. Manually review evidence and the applicable scenarios as well. No audit command mutates policy or grants permission.

## Distinct audit dimensions

Use these dimensions to avoid blind spots: instruction authority; scope; source coverage; source trust; freshness; conflicts; feasibility; permissions; target identity; privacy; dependency integrity; reproducibility; compatibility; input validation; numerical correctness; idempotency; concurrency; failure handling; cancellation; backup coverage; restore behavior; rollback; resource limits; real output quality; artifact contents; delivery state; citation fidelity; user communication; review-tool correctness; and policy drift.

Apply the relevant dimensions. “Not applicable” is a documented judgment, not a successful test. A larger check count does not make an audit better.
