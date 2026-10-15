# Integrated instruction audit — 10-15-2026

Policy version: 1.1.0. Scope: the other agent's published 0bdc6fe modules and implementation, current explicit mandatory-Rust instructions, the integrated full guide, 45-rule verification register, Rust tooling, and self-check procedures. Initial reports are preserved in [the historical audit](archive/INITIAL_INSTRUCTION_AUDIT.md) and [historical validation](archive/INITIAL_VALIDATION.md); their Python results do not validate this revision.

## Material findings

| Finding | Severity | Correction and evidence | Remaining limit |
| --- | --- | --- | --- |
| INT-001 | High | Python maintenance/adoption code and language exceptions contradicted the current explicit user instruction. Replaced executable implementation/tests with Rust; removed the exception from ENG-001, EXEC-002, PREF-003, adapters, policy metadata, and active docs. | Inherited third-party dependency sources are not automatically rewritten. |
| INT-002 | Medium | Separate drafts could lose one rule family or advertise only a partial guide. Preserve all 125 existing IDs plus 45 verification IDs; Rust builds a single complete GUIDEBOOK and validates source order, unique IDs, audit IDs, adapters, and digest lock. | Structure checks cannot prove complete semantic truth. |
| INT-003 | Medium | Old validation reports and initial-source visibility could be mistaken for current evidence. Archive original reports and identify the later published revision; current validation is reported separately. | Unpublished worker state and exhaustive historical-message coverage remain unavailable/unreviewed. |
| INT-004 | Medium | File hashes alone could be mistaken for reading/compliance. Require concrete application notes, all-rule dispositions, real evidence references, and task acceptance conditions. A generated record starts blocked. | Honest reading, evidence quality, and live-model behavior need semantic/empirical review. |
| INT-005 | Medium | Original maintenance instructions did not model recurring policy review or invalidate changed reading receipts. Add explicit review deadlines, weekly freshness CI, and hash mismatch rejection. | A schedule detects metadata drift; it does not autonomously update policy. |

## Semantic review

Reviewed both source families for authority/scope, current user corrections, privacy, evidence, finite audit completion, actual loading, and changed-input invalidation. A separate read-only review identified stale generated adapters, competing source-edit instructions, the obsolete language exception, and the need to retain stable-toolchain/portable-configuration guidance. Those findings guided the integrated correction.

The existing AUD-001 through AUD-020 cover instruction retrieval, contradiction/staleness checks, authority, evidence, behavioral probes, findings, correction, and bounded completion. The additional verification rules provide scoped observable checks and receipt schema. Domain-specific requirements stay scoped. Adoption tests use synthetic temporary Git projects; they do not configure an external provider or prove model obedience.

## Completion and evidence boundary

Current executable commands, exact counts, toolchain, artifacts, and limitations are in [VALIDATION.md](VALIDATION.md). Before publication, rerun the final-revision checks and inspect remote CI. Audit completion means examined areas and corrected findings are evidenced or explicitly limited; it does not mean every historical private message was read, every live platform was tested, or future technology is frozen.
