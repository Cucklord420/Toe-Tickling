# Supplemental guidebook audit — 10-15-2026

## Scope and evidence boundary

Reviewed the public default branch `main` at commit `d3e4218edac6f54a07407a7cee7a6331a0d8b538`, version 1.1.0 with 153 numbered rules, its source modules, generated adapters, installer, tests, license and workflows. Inspected GitHub repository/branch/ruleset metadata and completed workflow-run conclusions. The existing main-branch workflow passed twice on that commit; those historical results do **not** validate this proposed change. No provider-specific model-obedience test or real-world adoption test in the user's other repositories was performed.

This is a manual semantic/security audit supported by selected automated regression cases. Passing a regression is not proof that an LLM will follow a written rule, that a historical credential is safe, or that a mutable remote URL identifies an approved revision.

## Findings register

| Finding | Severity | Evidence / impact | Mitigation and verification |
| --- | --- | --- | --- |
| SUP-001 | **High** | GitHub returned `protected: false` for `main` and an empty repository ruleset list. This guide is a high-trust instruction supply-chain dependency: a direct edit or account compromise can change what later agents read. | **Open / owner settings required.** Protect `main`, require the named validation check and independent review for substantive policy changes where available, disallow force pushes/deletion and review bypasses. Verify actual settings and a rejected unreviewed mutation. |
| SUP-002 | **Medium** | Original validator only inspected `.github/workflows/validate.yml`. A newly added `.yaml`/`.yml` workflow could evade this check. Even in the original workflow, token-permission checking relied on substring presence. | **Code/test fix included in this unmerged branch.** Inspect all workflow definitions and reject absent/overriding permissions, write scopes, dangerous privileged events, secret references and non-SHA action pins. Added adversarial tests. These text checks still do not parse full YAML or replace code review. |
| SUP-003 | **Medium** | Original credential scanner recognized only a few prefix patterns, not examples of newer token families such as `sk-proj-` or GitLab/Slack formats. A passing scan could give false confidence before public release. | **Code/test fix included in this unmerged branch.** Add selected high-signal prefixes and synthetic negative tests. **Residual:** pattern matching cannot guarantee absence of personal data, arbitrary credentials, or secrets in prior Git history. |
| SUP-004 | **Medium** | Rule ID extraction matched bold ID strings anywhere in source text. A prose mention could count as a required audit rule even if no operational rule declaration existed. | **Code/test fix proposed.** Count only bullet-form numbered declarations, with an adversarial missing-rule test. **Residual:** semantic quality still requires human/model review. |
| SUP-005 | **Medium** | `AGPL-3.0-only` is selected for both program and guide text, and adoption copies the full text into unrelated repositories. A copied document remains licensed; a snapshot is not necessarily a relicensing of the surrounding software. Documentation licensing and redistribution requirements deserve independent legal review before large-scale adoption. | **Open / policy decision.** Preserve the existing AGPL choice without unilateral relicensing. Document third-party redistribution, copied notice/license obligations, and assess a distinct documentation license only by explicit copyright-holder decision. |
| SUP-006 | **Medium** | `policy_version` and an unpinned default-branch URL are distinct from a reviewed commit. Renaming the branch or changing content without bumping the human version can confuse caches, snapshots, and model handoffs. Digest matches prove byte identity to a source, not remote ownership or approved meaning. | **Open / design.** For high-trust deployments use an explicitly reviewed commit SHA plus the file digest; check freshness and adoption-lock drift, and require policy version/change history updates in review. |
| SUP-007 | **Medium** | The public profile includes first name, general NYC area, retail repair work, and detailed stable preferences. It omits sensitive/private records, but combining these attributes with a public GitHub identity could increase identifiability. | **Open / owner privacy decision.** Review each preference for public necessity, never auto-import private memory, and verify repository history before publication. No private details were added by this audit. |
| SUP-009 | **Medium** | Independent Rust checker counted bold IDs mentioned in prose, allowing a different spoof from actual bullet-form rules. | **Code/test fix included in this unmerged branch.** Require canonical rule declarations and add negative fixtures; verify real Rust compilation and check at the final commit. |
| SUP-010 | **Medium** | The 100-pass runner is stability/reproducibility testing, not one hundred independent legal, semantic, or live model reviews. | **Test harness included.** Count separate `check`, full Python regressions, and native Rust checker executions, log exact tested SHA, and mark untested layers. |
| SUP-011 | **Medium** | CI validation remains regex/text-based; complex YAML syntax, anchors, dynamically constructed expressions, and changes in event/permission semantics may evade simplified checks. | **Open.** Use full GitHub security settings and periodic workflow review; independently inspect workflow AST/provider execution where available. No claim of hermetic CI enforcement. |
| SUP-012 | **Medium** | GitHub branch protection/rulesets are absent and no repo-specific evidence was obtained for push-protection alerts or enforcement configuration. | **Open / settings change requires a supported authorized action.** Enable and verify protection and inspect GitHub's actual supported secret scanning. |
| SUP-008 | **Medium** | The various Claude/Gemini/Copilot/Cursor/OpenCode adapters request a full read, but neither their existence nor Python unit tests demonstrate live cross-provider loading, rule precedence, long-context completeness, or adherence. Existing adapters in adopted repos are deliberately preserved and may not load the guide. | **Open / empirical.** Test each actual provider/runtime with a synthetic instruction conflict, an untrusted injected document, a withheld private detail, and the required start/final audit. Record product version, retrieved source digest, and observed response; mark untested providers unknown. |

## Required self-check matrix

1. **Integrity:** `python3 scripts/guidebook.py check` and generated-source/lock drift; count declarations rather than mentions.
2. **Regression:** `python3 -m unittest discover -s tests -v`; review actual process exit code and failed scenarios, not only a self-reported count.
3. **Workflow isolation:** add a synthetic insecure second workflow and a conflicting permission block; verify both are rejected, then verify a safe additional workflow passes.
4. **Credential boundary:** add synthetic secret-pattern fixtures in a disposable copy; confirm rejection without printing or committing real secrets.
5. **Installer:** preview and apply only in a disposable Git repo; test existing project instructions, symlinks, path traversal, partial failure, source locks, idempotency, and diff preservation.
6. **LLM behavior:** live provider tests for loading, privacy, version, prompt-injection resistance, permission boundaries, and inability to claim unsupported writes.
7. **Licensing/privacy:** inspect additions and Git history before every public merge; run a separate dependency/license/notice review on adoption targets.
8. **Freshness/authority:** resolve the canonical repository default branch and exact commit, compare against locally pinned snapshot, and reject unreviewed instruction changes.
9. **Release gate:** re-run checks on final PR head **and** verify the resulting GitHub Actions status; a historical green run cannot authorize a different commit.
10. **Audit the auditor:** deliberately weaken one check in a test fixture and make sure another independent test detects the regression. Do not infer behavior from documentation or the test author’s assurance.

## Status definitions

`proposed`: code/test changes exist on an unmerged review branch; `verified`: checks completed on an identified revision; `blocked`: unsupported permissions/provider access; `open`: unresolved design/owner decision; `not tested`: no empirical evidence. Record new source digests, the PR head SHA, and CI conclusion in the merge review.

## Primary references

- GitHub — reviewed 10-15-2026 — https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches
- GitHub — reviewed 10-15-2026 — https://docs.github.com/en/actions/reference/security/securely-using-pull_request_target
- GNU — reviewed 10-15-2026 — https://www.gnu.org/licenses/gpl-faq.en.html


## Legal clarification after owner's confirmation

The owner reconfirmed a **copyleft-for-public-work** policy, not an instruction to waive copyright or change the existing license. U.S. Copyright Office guidance (01-29-2025) distinguishes unprotectable purely AI-generated expression from sufficiently human-authored creative selections, arrangements and modifications of AI-assisted material. GNU describes copyleft as copyright-dependent. Existing AGPL-3.0-only is preserved without declaring all AI output copyrightable. See [COPYLEFT_AI_LEGAL_REVIEW.md](COPYLEFT_AI_LEGAL_REVIEW.md). This analysis does not establish actual human authorship, ownership, derivative-work status, or court enforceability for each file.

## New v1.1.0 native Rust audit

Current main contains a separate Rust read-only validator and its tests. This branch fixes a semantic spoofing gap in its rule-ID recognition. Python remains the reference generator/installer; Rust is an additional check, not a parity-complete replacement. Validate the exact final branch with real Rust compilation and unit tests; a source inspection or a historical CI badge is insufficient.
