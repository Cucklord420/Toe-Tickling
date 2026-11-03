---
name: parallel-agent-worktrees
description: Use when two or more coding agents or workers operate on a GitHub repository or when considering native git worktrees or Worktrunk. Keep branch isolation separate from logical ownership claims and validate each merge against exact-head CI, reviews and protections.
license: AGPL-3.0-only
---

# Safe parallel-agent Git worktrees

**One worktree is not one atomic task claim.** A worktree isolates files; repository-specific Issue, ref reservation and PR rules determine who owns a change.

## Claim and scope

1. Read the current default branch SHA, repository AGENTS.md, issue/PR discussion, branches, actual coordination/lock rules, CI, reviewers and working-tree state.
2. Reuse canonical existing artifacts and check overlap by root cause, requirements, changed paths, tests and history—not title alone. Do not create an Issue or PR just to record another redundant status.
3. Acquire the task's authorized **atomic** reservation where applicable. A comment is not an atomic lock when Git ref claims are required.
4. Use one writer per authorized branch and one isolated worktree per active task. [Worktrunk](https://github.com/max-sixty/worktrunk) is an optional ergonomic tool, not a requirement and not permission to clean up, merge or override another agent's branch.

## Implement and validate

- Before each write refresh the target ref, valid claim, base/head SHAs and competing work. Use expected-SHA safeguards; never force-push, reset unrelated changes or modify a protected branch directly.
- Make the smallest coherent change. Exercise real entry points, error paths, data preservation, rollback, platform compatibility and required tests. Record exact commands and final commit.
- Review the full exact-head diff, privacy, license, open threads, approvals and required CI on that SHA. An earlier green run is stale after another commit.
- Merge only when checks, approval rules, conflict state, authorization and concurrency gates are met; re-fetch actual merged state and target-branch SHA. A merged PR is not a deployment.

## Failure scenarios

- Two worktrees claim the same task: stop duplicate writers until the canonical reservation is resolved.
- A new push invalidates CI: revalidate updated head; do not use old green results.
- A Draft PR has unresolved work: do not promote on cosmetic checks alone.
- A PR is blocked: preserve its work and continue another independent eligible task.

No automatic merge, service deployment, branch deletion or remote security change is authorized by this skill.

## Self-checks

Verify a real task reservation separately from distinct worktrees. Before every write compare canonical issue, branch and expected SHA; before merge check full diff, exact-head CI and actual required approvals. Simulate duplicate ownership, stale checks, changed refs and failed post-merge readback as negative cases.
