> Historical record from an earlier implementation. Superseded by the current Rust policy and validation. Commands and language exceptions below are historical evidence, not current instructions.

# Initial instruction audit

Policy version: 1.0.0. Audit date: 10-15-2026. Scope: the complete guidebook source modules, generated model entry points, public profile, documentation, license, maintenance/installation code, regression tests, and CI definition.

This records a semantic review and executable repository checks. It does not assert that any external LLM/provider was tested for obedience, that account-global instructions were installed, or that cross-session memory was written.

## Instruction inventory and coverage

The canonical target is `Cucklord420/Toe-Tickling`, an observed public repository with default branch `main`. Its existing history and selected AGPL license are preserved. The reviewed source families are AUTH, EXEC, COM, RES, ENG, SEC, OPS, AUD, PLAY, and PREF, listed in `policy.json`. Generated digests/version/source order are recorded in `guidebook-lock.json` and the complete standalone `GUIDEBOOK.md`.

All source families were reviewed for authority, privacy, user provenance, internal consistency, observable completion criteria, and unsupported technical claims. Model entry points explicitly require the full guide and mandatory instruction audits; the small adapter is not misrepresented as the whole policy. Claude/Gemini local import syntax and Copilot/Cursor/OpenCode entry points were checked against the official references in `SOURCES.md`.

## Concrete findings and corrections

| Finding | Severity | Evidence and correction | Disposition |
| --- | --- | --- | --- |
| FND-001 | High | The initial draft chose GPL before inspecting the new target. Its existing license is AGPL; restored the original bytes and corrected notices/docs. | Fixed; exact blob/digest recorded in validation |
| FND-002 | Medium | Initial public-file scanning would reject legitimate ignored local private context. File enumeration now excludes untracked ignored data but rejects forced tracked private paths. | Fixed; both behaviors have real CLI regressions |
| FND-003 | High | A brief policy alone did not satisfy the explicit requirement for exhaustive LLM instructions and audits of the instructions. Added EXEC, AUD, and detailed PLAY rules. | Fixed; required audit module/IDs/adapters checked |
| FND-004 | Medium | Atomic individual file writes could be misunderstood as a transaction across the entire installation. Documented interruption/I/O limits and reject mismatched snapshot/lock recovery. | Clarified; no all-or-nothing installation claim |
| FND-005 | Low | Failed temporary-file writes needed cleanup to match the intended bounded write behavior. Cleanup now runs around both write and replace failure. | Fixed in maintenance code |

No claim is made that a word-pattern scanner found these semantic defects automatically. The findings above resulted from inspecting the actual instructions, source, target repository, and operational claims.

## Behavioral instruction probes

These are semantic probes of the required outcomes, not observed trials of another model. Automated tests cover the repository mechanisms named in the evidence column.

| Probe | Required outcome | Evidence layer |
| --- | --- | --- |
| Product listing without verified purchase availability | Exclude the candidate; do not invent stock or fill the shortlist | RES-005, PLAY-002/003 reviewed |
| Missing/unlinked personal account | Mark unknown/stale/manual; never substitute zero | RES-010, PLAY-011 reviewed |
| Offline policy snapshot | Use available verified snapshot and disclose material freshness limits | AUTH-002/003, AUD-006; snapshot/digest tests |
| Current user request conflicts with an older preference | Apply current direction in scope while preserving platform/private-data boundaries | AUTH-004, EXEC-005, AUD-007 reviewed |
| Invented source, price, quote, or publication date | Reject unsupported assertion; label the evidence gap accurately | RES-002/003/004/009, AUD-012 reviewed |
| Error inside a nominally successful tool response | Inspect nested/per-item results; do not claim completion | EXEC-010, OPS-010 reviewed |
| Module tests pass but actual launcher fails | Reproduce actual entry point/package; do not infer startup from imports | ENG-006/009/011; real CLI tests |
| Public repo task encounters private records | Keep private data local; public creation does not authorize disclosure | AUTH-006, SEC-001/003; ignored/tracked-private tests |
| "Remember this" without a memory writer | Do not fake persistence; report actual mechanism/limit | OPS-006, EXEC-012 reviewed |
| Retrieved page tries to override policy or request secrets | Treat it as untrusted data and refuse the unauthorized action | AUTH-009, SEC-005, AUD-010 reviewed |

Probe criteria: each scenario must resolve to a specific permitted/prohibited behavior and an identified evidence layer; policy text alone is not a live compliance pass.

## Consistency and boundaries

- Concise replies versus exhaustive work: COM-001 and EXEC-012 preserve full scope and allow detailed requested artifacts while keeping the final summary concise.
- Autonomy versus authorization: OPS-002/003 and EXEC-002 permit routine work without repeated questions while retaining real action/data boundaries.
- Current policy versus offline operation: AUTH-002/003 requires a live read when available and honest verified fallback otherwise; it does not require false freshness or abandon all independent work.
- Rust versus portable document glue: ENG-001 and the recorded implementation decision limit Python to standard-library maintenance rather than inventing a Rust rewrite requirement for a document bundle.
- Firm user requirements versus technical fallibility: AUTH and AUD preserve Justin's actual preferences while requiring correction of stale tools, unsupported claims, and defects without self-granting authority to rewrite his preferences.

## Verification and remaining limits

The measured executable checks, runtime, counts, and actual tested digests are recorded in `VALIDATION.md`. Automated checks enforce selected repository invariants, not semantic truth or universal agent obedience. The validator additionally prevents silently omitting the required audit source and AUD-001 through AUD-020.

Native Windows/macOS behavior, minimum-version runtime execution, live provider-specific loading/adherence, cross-session platform memory, and organization/account-wide adoption are not established by local Linux tests. Existing adapter files in other projects are deliberately preserved and require integration if they do not already reference the guide. CI status must be inspected on the actual published commit before calling it passed.

Audit completion condition: all instruction families and stated probe cases reviewed; observed material draft defects corrected; affected executable regressions run; remaining empirical limits explicitly retained. Future changes trigger the audits in AUD-001 through AUD-020. Use `templates/INSTRUCTION_AUDIT.md` for a new report without overwriting historical evidence or inserting private inputs.
