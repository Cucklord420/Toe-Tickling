# Historical Python and current Rust CLI coverage

## Reference and scope

The last published Python implementation before the Rust migration is pinned at commit [`0bdc6fed00c35cec4ff3bf756da9d1f9dbb0c3a9`](https://github.com/Cucklord420/Toe-Tickling/commit/0bdc6fed00c35cec4ff3bf756da9d1f9dbb0c3a9). Its `scripts/guidebook.py` selected `ROOT = Path(__file__).resolve().parents[1]`, so invoking that script by path from an unrelated current directory still selected the script's checkout. The revision contains 45 Python test methods in `tests/test_guidebook.py`.

The Rust CLI now defaults to the package manifest directory through Cargo's compile-time `CARGO_MANIFEST_DIR`; `--root PATH` still selects another checkout. A Rust integration test launches the real binary from a fresh unrelated directory and exercises `build`, `check`, and read-only `adopt` preview. It also verifies that these commands do not create files in their launch directory. Existing tests continue to exercise explicit `--root` selection and adoption paths containing spaces.

The mapping below compares historical scenarios with current Rust source and test coverage. A mapped scenario is not a claim that the historical Python and Rust tests ran together, emitted identical output, or passed on the same platform.

| Pinned Python test | Current Rust counterpart or status | Coverage assessment |
| --- | --- | --- |
| `test_real_cli_build_and_check_from_unrelated_directory` | `tests/validation.rs::build_and_check_default_to_manifest_root_from_unrelated_directory` | Direct real-CLI regression for `build`, `check`, and also `adopt` preview from an unrelated directory; exact-head CI is still required. |
| `test_build_is_deterministic` | `tests/validation.rs::full_guide_and_adapters_cannot_drift`; current render and source-byte checks | Partial: verifies generated content against canonical sources, but there is no identical-tree before/after snapshot test for two CLI builds. |
| `test_generated_guide_tamper_is_rejected` | `tests/validation.rs::full_guide_and_adapters_cannot_drift` | Current checker rejects stale generated guide content. |
| `test_generated_adapter_tamper_is_rejected` | `tests/validation.rs::full_guide_and_adapters_cannot_drift` | Current checker rejects a stale generated adapter. |
| `test_missing_generated_guide_is_rejected` | `tests/validation.rs::full_guide_and_adapters_cannot_drift` | Partial: the current checker reads and validates generated outputs, but the named regression mutates content rather than deleting the file. |
| `test_changed_source_requires_rebuild` | `tests/validation.rs::canonical_change_requires_render` | Covered at the check/render boundary; this does not compare Python and Rust bytes. |
| `test_invalid_date_is_rejected` | `tests/validation.rs::policy_date_uses_new_york_calendar_across_summer_and_winter_midnight`; typed policy validation | Partial: Rust has calendar-boundary coverage; no matching fixture for the old CLI's invalid `policy.json` date error text. |
| `test_invalid_version_is_rejected` | Rust policy loading and validation in `src/guide.rs` / `src/lib.rs` | Gap: no one-to-one invalid-version CLI fixture or error-code comparison is identified. |
| `test_duplicate_rule_id_is_rejected` | `tests/validation.rs::removing_existing_rules_or_restoring_language_exception_fails`; `guide::build` validation | Partial: stable-ID retention and invalid IDs are exercised; a duplicate source-rule fixture is not the same case. |
| `test_missing_source_is_rejected` | `tests/validation.rs::missing_intake_document_fails`; generated-source validation | Partial: required intake-file removal is covered; a missing `policy.json` source entry is not an exact counterpart. |
| `test_source_traversal_is_rejected` | `tests/security.rs::dot_and_parent_segments_cannot_hide_a_root_symlink_prefix`; `guide::safe_path` | Partial: path containment and symlink ancestry are covered, but the historical malformed-source CLI case is not run differentially. |
| `test_audit_module_cannot_be_silently_omitted` | Required rule preservation and source-bundle checks in `tests/validation.rs` | Partial: required rule IDs are retained; no same-input Python/Rust source-list omission fixture is run. |
| `test_mandatory_audit_rule_cannot_be_removed` | `tests/validation.rs::removing_existing_rules_or_restoring_language_exception_fails` | Covered for stable-ID removal; CLI wording and exit codes are not compared. |
| `test_all_adapters_include_mandatory_instruction_audit` | `tests/validation.rs::full_guide_and_adapters_cannot_drift`; Rust adapter generation | Mechanical output is checked; live model loading is not established. |
| `test_source_symlink_is_rejected` | `tests/security.rs::public_symlink_files_directories_and_root_ancestors_are_rejected` | Covered by Rust symlink rejection tests where the platform supports creating the fixture. |
| `test_broken_local_link_is_rejected` | `tests/security.rs::markdown_link_forms_reject_missing_and_escaping_destinations` | Covered by Rust Markdown link validation. |
| `test_escaping_local_link_is_rejected` | `tests/security.rs::markdown_link_forms_reject_missing_and_escaping_destinations`; `dot_and_parent_segments_cannot_hide_a_root_symlink_prefix` | Covered by Rust containment checks; not an output-parity test. |
| `test_secret_like_token_is_rejected` | `tests/security.rs::selected_credentials_and_contacts_fail_without_leaking_matches` | Covered by current Rust credential checks; patterns have evolved. |
| `test_private_contact_pattern_is_rejected` | `tests/security.rs::selected_credentials_and_contacts_fail_without_leaking_matches` | Covered by current Rust contact checks; patterns have evolved. |
| `test_private_path_is_rejected` | `tests/security.rs::tracked_private_paths_remain_rejected_even_when_ignored`; `unignored_untracked_private_context_is_rejected` | Covered by tracked and untracked private-path checks. |
| `test_gitignored_local_context_is_not_read_as_public_content` | `tests/security.rs::git_ignored_local_context_is_never_scanned`; `tests/validation.rs::clean_source_tree_allows_audits_with_ignored_private_and_build_outputs` | Covered by Rust Git-aware scan tests. |
| `test_forced_tracked_private_file_is_rejected` | `tests/security.rs::tracked_private_paths_remain_rejected_even_when_ignored`; staged-content checks | Covered by Rust tracked/index privacy checks. |
| `test_mutable_action_pin_is_rejected` | `tests/security.rs::every_job_and_reusable_workflow_keeps_permission_and_pin_controls` | Covered by current Rust workflow validation. |
| `test_writable_ci_is_rejected` | `tests/security.rs::ci_controls_cannot_be_satisfied_by_comments_or_another_checkout`; workflow permission tests | Covered by current Rust workflow validation. |
| `test_preview_does_not_write` | `tests/adoption.rs::preview_is_read_only_and_contains_full_snapshot` | Covered by a filesystem snapshot of the target before and after planning. |
| `test_install_creates_full_snapshot_and_matching_digest` | `tests/adoption.rs::apply_installs_matching_snapshot_and_all_model_entry_points` | Covered by installed-byte, digest-lock, and entry-point assertions. |
| `test_existing_project_instructions_are_preserved` | `tests/adoption.rs::managed_blocks_preserve_exact_project_prefix_and_suffix` | Covered for project-owned content before and after the managed block. |
| `test_existing_gitignore_is_preserved` | `tests/adoption.rs::managed_blocks_preserve_exact_project_prefix_and_suffix` | Covered for existing ignore rules and the managed private-file exception. |
| `test_repeat_install_is_byte_identical` | `tests/adoption.rs::repeat_install_is_byte_identical` | Covered by a target-tree byte snapshot. |
| `test_existing_adapter_is_never_overwritten` | `tests/adoption.rs::existing_model_adapters_are_preserved_including_non_utf8_content` | Covered for existing adapter contents, including invalid UTF-8. |
| `test_malformed_marker_refuses_all_writes` | `tests/adoption.rs::malformed_or_duplicate_markers_refuse_the_entire_install` | Covered by all-or-nothing rejection. |
| `test_reversed_markers_refuse_all_writes` | `tests/adoption.rs::malformed_or_duplicate_markers_refuse_the_entire_install` | Covered by malformed managed-block rejection. |
| `test_symlink_snapshot_directory_refuses_writes` | `tests/adoption.rs::symlink_targets_ancestors_and_managed_paths_are_rejected`; `symlinks_introduced_after_preview_are_rejected` | Covered on platforms where the symlink fixture is supported; exact Windows behavior requires current CI evidence. |
| `test_symlink_adapter_refuses_all_writes` | `tests/adoption.rs::symlink_targets_ancestors_and_managed_paths_are_rejected` | Covered on platforms where the symlink fixture is supported. |
| `test_symlink_target_ancestor_is_rejected` | `tests/adoption.rs::symlink_targets_ancestors_and_managed_paths_are_rejected` | Covered on platforms where the symlink fixture is supported. |
| `test_tampered_existing_snapshot_is_rejected` | `tests/adoption.rs::snapshot_tampering_after_preview_blocks_apply`; `modified_snapshot_and_invalid_or_foreign_lock_are_rejected` | Covered by current Rust snapshot/lock checks. |
| `test_unmanaged_snapshot_is_rejected` | `tests/adoption.rs::modified_snapshot_and_invalid_or_foreign_lock_are_rejected`; `incomplete_snapshot_pairs_are_never_replaced` | Covered by Rust refusal to overwrite unmanaged or incomplete snapshots. |
| `test_non_repository_target_is_rejected` | `tests/adoption.rs::non_git_missing_and_nested_targets_are_rejected_without_creating_files` | Covered with no-write assertions. |
| `test_subdirectory_target_is_rejected` | `tests/adoption.rs::non_git_missing_and_nested_targets_are_rejected_without_creating_files` | Covered with no-write assertions. |
| `test_self_adoption_is_rejected` | `tests/adoption.rs::self_adoption_and_targets_inside_guidebook_are_rejected` | Covered by Rust target-boundary validation. |
| `test_non_directory_parent_refuses_all_writes` | `tests/adoption.rs::non_directory_parents_and_file_type_collisions_block_all_writes` | Covered by target snapshot comparison after refusal. |
| `test_missing_target_is_not_created` | `tests/adoption.rs::non_git_missing_and_nested_targets_are_rejected_without_creating_files` | Covered; the missing path remains absent. |
| `test_snapshot_update_preserves_unrelated_project_rules` | `tests/adoption.rs::source_update_replaces_managed_snapshot_and_preserves_project_rules` | Covered for source refresh and preservation of project-owned instructions. |
| `test_installer_does_not_commit` | `tests/adoption.rs::apply_installs_matching_snapshot_and_all_model_entry_points`; `tests/validation.rs::adoption_cli_runs_from_an_unrelated_directory_and_path_with_spaces` | Covered by verifying adoption leaves the target without a Git commit. |
| `test_atomic_replace_failure_preserves_original_and_cleans_temporary` | `src/guide.rs::build_transaction_tests::later_persist_failure_restores_existing_and_absent_outputs`; adoption rollback tests | Partial: Rust verifies multi-output rollback and recovery; it does not reproduce Python's monkeypatched single-file `Path.replace` failure. |

## Historical execution evidence and current limits

- `docs/archive/INITIAL_VALIDATION.md` records a Python 3.12.14 Linux run on 10-15-2026 with all 45 tests passing and no skips. The pinned commit contains the matching 45 test methods.
- The archived report's maintenance-script SHA-256 (`e80694f6d6a0a3686c79eed8b10da0892394f5f0d65f04f423cf815b4dc28552`), test-source SHA-256 (`76a318489752c80f28dc7980e25a6ed27b8e6ea221ac1ffb624a048974dfb6b4`), and generated-guide SHA-256 (`7e7ea0e5fd53fb6d8e89bf8c7a72b915dc3c555d2af9c949412d75b2aadcf887`) were recomputed from the pinned commit's extracted tree and match the report. This ties the historical 45-test result to the selected source snapshot.
- Four historical symlink tests use a Windows skip decorator. This implies an expected Windows count of 41 executed and 4 skipped, but no Windows result is claimed here: neither that archived report nor this session contains a Windows run of the pinned Python suite.
- This Windows authoring environment has Rust 1.99.1 and Cargo 1.99.1, with Rust edition 2024 pinned, but Cargo tests stop before test execution because `link.exe` is missing. The new regression has therefore not run locally.
- No usable Python interpreter is installed in this environment, so the pinned Python suite was inspected but not rerun. Its previous 45-test pass remains historical evidence only.
- The repository workflow runs the current Rust checks on Ubuntu and Windows; its results apply only to the exact PR head reported by GitHub. There is no native macOS run in the current workflow.

## Intentional migration differences

The approved design accepts these behavioral differences as migration scope, not as byte-for-byte parity:

- The current Rust policy and generated bundle have grown since the Python snapshot. Generated guide/adapters and their digests are expected to differ.
- The Python `check` command printed a JSON report; the Rust `check` command prints human-readable structural-check output. Exact terminal text and error formatting are not golden-compatible.
- Rust adds `render`, `refresh-manifest`, `new-audit`, and adoption recovery. These commands are additions, not Python parity requirements.
- The source-root behavior is intentionally restored: default commands select the package manifest directory, while `--root PATH` is retained for another checkout.

The root-selection regression is a targeted compatibility repair. Full output, return-code, warning, refusal, and filesystem-side-effect differential fixtures across every supported platform remain unfinished; Issue #23 stays open until its broader acceptance criteria have adequate evidence.
