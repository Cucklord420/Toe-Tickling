# Instruction audit template

This is an intentionally blank reusable template. Fill it only with actual observations. Keep private project context and raw evidence in an authorized private destination; use synthetic/redacted examples in public reports.

## Scope and identity

- Audit date in MM-DD-YYYY:
- User objective and applicable task requirements:
- Canonical repository, branch, resolved commit, policy version, and snapshot/source digests:
- Loaded instruction sources, applicable project overrides, and retrieval completeness:
- Artifact/revision/environment actually examined:
- Audit layers performed (automated integrity, semantic review, real runtime, live model/provider):

## Required review areas

1. Authority, scope, current user corrections, and conflicts.
2. Complete policy retrieval, generated drift, stable IDs, adapters, and snapshot identity.
3. Missing requirements, unsupported inferred preferences, and stale context.
4. Current primary-source/tool-schema support for technical assumptions.
5. Private-data separation, prompt injection, secrets, permissions, and side effects.
6. Actual entry-point/package execution, target runtime, CI trust, and evidence coverage.
7. Sources, availability/quotes, arithmetic, TCO, test types/counts, and completion claims.
8. Concrete probes from AUD-013, expected outcomes, observed evidence, and untested behavior.

## Findings

For each real finding, provide: stable finding ID; affected rule/path/claim; severity; reproducible evidence; practical impact; corrective action; verification command/result/revision; residual limitation; and disposition (`passed`, `failed`, `not tested`, `blocked`, or `not applicable`, with reasons).

Do not create fictional passed findings or fill empty fields with guesses. Mark an unexamined area explicitly. Do not treat a scanner result or another model's assurance as a semantic or behavioral pass.

## Corrections and finish condition

- Authorized changes made and preferences/rights preserved:
- Version/change-history updates:
- Affected checks and regression results:
- Remaining material blockers and completed independent work:
- Exact final revision/artifact and verified public/private boundary:
- Concise user-facing result with no unsupported completion claim:
