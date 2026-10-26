# Integrated instruction audit and follow-up findings

## Follow-up — 10-16-2026, policy 1.2.3

Base inspected: `4f519c3ea1ed120d36aefea014fbf31bc0593304` (policy 1.2.2). Published PR #6 at `87098798d02587a6e8d3eb683760d91c79894433` remains a historical, separate audit; its Python/100-run evidence does not validate this change. This review does not establish access to another worker's unpublished files or continuous supervision.

**EV-001 — Medium: stable verification IDs could disappear.** The primary `validate_policy` accepted removal of an existing rule as long as the remaining records were valid. The new removal regression failed before repair with `silently deleted AUTH-01`. Consistently regenerating the rendered book and hashes could therefore hide lost obligations from the primary structural check. The independent auditor guarded the older baseline but omitted the later ENG-020 and ENG-07 obligations.

Correction: require all 46 existing verification IDs in the primary validator and update the independent auditor to all 159 standing and 46 verification IDs. Preserve explicit supersession instead of requiring that every historical rule remain active. Regressions remove each current verification ID, accept a valid retained-history replacement, remove ENG-020 from a rebuilt synthetic bundle, and remove ENG-07 with consistent rendered bytes/counts/digests. Scenario S29 and the self-check procedure describe this failure mode. Check results for this revision belong in [VALIDATION.md](VALIDATION.md).

Residual limits: minimum-ID guards require deliberate maintenance when adding future rules; retaining an ID cannot detect every semantic weakening of its text. Review source diffs and authority/provenance as well as tool results. No private corpus was republished, user preference changed, provider configured, or live model compliance established by this repair.

## Original integrated review — 10-15-2026

Policy version: 1.2.0. Scope: the other agent's published 0bdc6fe modules and implementation, current explicit mandatory-Rust instructions, the integrated full guide, 45-rule verification register, Rust tooling, and self-check procedures. Initial reports are preserved in [the historical audit](archive/INITIAL_INSTRUCTION_AUDIT.md) and [historical validation](archive/INITIAL_VALIDATION.md); their Python results do not validate this revision.

## Material findings

| Finding | Severity | Correction and evidence | Remaining limit |
| --- | --- | --- | --- |
| INT-001 | High | Python maintenance/adoption code and language exceptions contradicted the current explicit user instruction. Replaced executable implementation/tests with Rust; removed the exception from ENG-001, EXEC-002, PREF-003, adapters, policy metadata, and active docs. | Inherited third-party dependency sources are not automatically rewritten. |
| INT-002 | Medium | Separate drafts could lose one rule family or advertise only a partial guide. Preserve all 158 existing IDs plus 45 verification IDs; Rust builds a single complete GUIDEBOOK and validates source order, unique IDs, audit IDs, adapters, and digest lock. | Structure checks cannot prove complete semantic truth. |
| INT-003 | Medium | Old validation reports and initial-source visibility could be mistaken for current evidence. Archive original reports and identify the later published revision; current validation is reported separately. | Unpublished worker state and exhaustive historical-message coverage remain unavailable/unreviewed. |
| INT-004 | Medium | File hashes alone could be mistaken for reading/compliance. Require concrete application notes, all-rule dispositions, real evidence references, and task acceptance conditions. A generated record starts blocked. | Honest reading, evidence quality, and live-model behavior need semantic/empirical review. |
| INT-005 | Medium | Original maintenance instructions did not model recurring policy review or invalidate changed reading receipts. Add explicit review deadlines, weekly freshness CI, and hash mismatch rejection. | A schedule detects metadata drift; it does not autonomously update policy. |
| INT-006 | Medium | An independent negative fixture showed that a symlink alias followed by `/.` bypassed an ancestry iterator. Validate lexical path prefixes before canonicalization in the audit, security checker, CLI, and adopter; retain the failing case as a Rust regression. | Protection from hostile filesystem swaps during a multi-file installation remains unverified. |

## Semantic review

Reviewed both source families for authority/scope, current user corrections, privacy, evidence, finite audit completion, actual loading, and changed-input invalidation. A separate read-only review identified stale generated adapters, competing source-edit instructions, the obsolete language exception, and the need to retain stable-toolchain/portable-configuration guidance. Those findings guided the integrated correction.

The existing AUD-001 through AUD-020 cover instruction retrieval, contradiction/staleness checks, authority, evidence, behavioral probes, findings, correction, and bounded completion. The additional verification rules provide scoped observable checks and receipt schema. Domain-specific requirements stay scoped. Adoption tests use synthetic temporary Git projects; they do not configure an external provider or prove model obedience.

## Completion and evidence boundary

Current executable commands, exact counts, toolchain, artifacts, and limitations are in [VALIDATION.md](VALIDATION.md). Before publication, rerun the final-revision checks and inspect remote CI. Audit completion means examined areas and corrected findings are evidenced or explicitly limited; it does not mean every historical private message was read, every live platform was tested, or future technology is frozen.

The final integration additionally retained all 28 EVO rules and five real-file-verification rules from 796ee24, corrected EVO-022's old language exception, and generalized the tracked Rust-build-output guard. Public-workflow validation checks every YAML workflow, including permissions, immutable action pins, credential persistence, bounded Rust command steps, privileged triggers, and secret contexts. Pattern checks cover selected modern credential formats without printing matched values. These are structural controls, not comprehensive proof of privacy or legal rights.
