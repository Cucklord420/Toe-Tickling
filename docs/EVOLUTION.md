# Keep the law alive

Nothing is static. Preserve durable intent while changing obsolete mechanisms. A review date is a prompt to investigate, not evidence that a rule is wrong or permission to weaken it.

## When to review

Recheck applicable guidance at every task intake. Review affected rules after a user correction, failed check, incident, tool/model/runtime change, changed API or license, source conflict, new technology, or newly discovered limitation. Review the complete register at least quarterly; initial calendar deadlines are 01-15-2027. The scheduled CI audit checks overdue metadata weekly. That schedule provides detection only; a human or authorized agent must do the semantic review.

Volatile release, stock, price, compatibility, deadline, and service-state claims need verification at the point of use even if the policy review date has not arrived. Keep those facts in dated project records rather than making them universal rules.

## Change protocol

1. Re-read current instructions and authorization. Capture the current policy commit and inspect concurrent work.
2. Identify the exact failed assumption or new requirement, source class, affected scope, and impacted rule IDs. Separate direct user statements from assistant suggestions, derived synthesis, and old context.
3. Propose the smallest coherent change with rationale, compatibility impact, risks, and verification. Do not silently replace a hard gate with a preference.
4. Edit `policy/rules.json` for normative rules. Preserve IDs across wording improvements. For replacement, retain the old rule with `status: superseded`, add the new active rule, and list the old ID in `supersedes`. Never reuse an ID for a different obligation. Record removed or narrowed requirements in the changelog.
5. Update operating procedures, scenarios, entry points, tests, provenance, and the changelog as needed. Render RULEBOOK.md, deliberately refresh the reading manifest, and run checks and regression tests.
6. Review meaning and evidence independently of the renderer and checker. A passing test suite cannot authorize a policy change or prove the change matches the user's intent.
7. Recheck upstream movement before integration. Deliver an isolated commit/PR unless existing instructions authorize another path. Reconcile the other agent's work without deleting either side's valid requirements.
8. Invalidate prior adoption receipts for changed files. Re-present the changed guidance and rerun affected model behavior cases before claiming the new version is active.

## Conflicts and exceptions

Instruction priority is defined by AUTH-01. Within that boundary, current explicit user decisions govern their subjects. More-specific compatible project rules add constraints. A source's date, authority, scope, and evidence status all matter: a fresh assistant summary cannot overrule an older explicit user requirement by timestamp alone.

Record an unresolved conflict, stop the dependent action, and continue useful independent work. For an authorized exception, record who authorized it, exact rule IDs and task scope, rationale, expiration, compensating checks, and residual risk in the private record. Do not edit the global rule merely to make one task pass. Exceptions never justify invented test results.

## Concurrent-agent review

Compare the base revision, current remote branch heads, PR changes, and local dirty files. Unpublished work is unavailable evidence. Use isolated branches or worktrees where appropriate, preserve unrelated edits, and do not force-push. If both agents changed the same obligation, merge the intent and rerun the applicable scenarios. Avoid duplicate corpus reviewers or live writes. Final reporting must identify the revision actually inspected, not imply continuous observation.

## Open-ended scope

A finite rule book cannot enumerate every future failure. Treat uncovered risks as an audit finding: record the gap, adopt a safe scoped approach under current authorization, and add a regression case when useful. Do not promise perfect obedience, infallibility, perpetual monitoring, or automatic deployment merely because the guide exists.
