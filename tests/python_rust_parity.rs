use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Component, Path, PathBuf},
    process::{Command, Output},
};
use tempfile::{Builder, TempDir};

const PYTHON_REFERENCE_COMMIT: &str = "0bdc6fed00c35cec4ff3bf756da9d1f9dbb0c3a9";
const PYTHON_REFERENCE_SCRIPT_SHA256: &str =
    "e80694f6d6a0a3686c79eed8b10da0892394f5f0d65f04f423cf815b4dc28552";
const PYTHON_REFERENCE_VERSION: &str = "Python 3.15.2";

const SENSITIVE_CHILD_ENV: &[&str] = &[
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "ACTIONS_RUNTIME_TOKEN",
    "ACTIONS_ID_TOKEN_REQUEST_TOKEN",
    "ACTIONS_ID_TOKEN_REQUEST_URL",
    "GIT_ASKPASS",
    "GIT_SSH_COMMAND",
    "GIT_SSH",
    "SSH_AUTH_SOCK",
    "SSH_AGENT_PID",
];

const GIT_CONTEXT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_TEMPLATE_DIR",
    "GIT_EXEC_PATH",
];

const PYTHON_ARTIFACT_GOLDENS: &[(&str, &str)] = &[
    (
        "GUIDEBOOK.md",
        "7e7ea0e5fd53fb6d8e89bf8c7a72b915dc3c555d2af9c949412d75b2aadcf887",
    ),
    (
        "guidebook-lock.json",
        "666e858a4fd30d8eff3850f555cec2a6a17773be3162282402d6e9909eae7b84",
    ),
    (
        "AGENTS.md",
        "ff60b430e9c090697491e500358fdb131c80d396a195565438e6b50a827b0970",
    ),
    (
        "prompts/USE_WITH_ANY_MODEL.md",
        "6c929562d6d33f771ba616a2243b92cac281b57f6648765a276f93b265a4ffcf",
    ),
    (
        "prompts/STANDING_MEMORY.md",
        "6b7de3c29b2129ea9bdf19efe595a34c92858835cf91cdf3b2e04c9d26ba7b5b",
    ),
    (
        "CLAUDE.md",
        "66c52d75f7765723fcf558fa47a9d76cc79f89dbd5e4da05ca5c71f36b153f3b",
    ),
    (
        "GEMINI.md",
        "6891d91ced7b50ab5b962caa0ce1e8374b41d748451d4f7af0ed2a4fa97bcb31",
    ),
    (
        ".github/copilot-instructions.md",
        "b2f1aee4d11b69b35970678e349de8d39d391eb594ad7c328300625d1f8e72b9",
    ),
    (
        ".cursor/rules/justin-guidebook.mdc",
        "397fa19a64cab31e001eee77d2f6576e8d04a7df455fe2f3165d0a3c89dd6aa3",
    ),
];

const RUST_ARTIFACT_GOLDENS: &[(&str, &str)] = &[
    (
        "GUIDEBOOK.md",
        "4123ddabf48b5efaa6f5fc4a138afd841094efe09b84fc74edcd1287a286388c",
    ),
    (
        "RULEBOOK.md",
        "71a2ee38eca98c45d1a178033fc216af7365a45eec8a9d107a9923151d2db652",
    ),
    (
        "guidebook-lock.json",
        "31f2f9142f887f26404a6b370696d9f5856ddb3eaa0984e748fba510f46bf69a",
    ),
    (
        "prompts/USE_WITH_ANY_MODEL.md",
        "84a43f97d374f07341c937cc393b0dce0e0380e062146fa2c645cc815a2b42a8",
    ),
    (
        "prompts/STANDING_MEMORY.md",
        "62a74370899f37e7fc25a4938ae0aeb842103cf6eae45391f0438a348860c635",
    ),
    (
        "CLAUDE.md",
        "8b7113d7777fa8411d3b3a3400d460a9c24a01e0ade2e4289a91519b0f81d1db",
    ),
    (
        "GEMINI.md",
        "cefdcf2dac212b67f9bd83d906e36dadbb58dd0692c9c655fb585d847ea64c2a",
    ),
    (
        ".github/copilot-instructions.md",
        "f48ce44f0e056ae559895ea72137cf0b0f08ea263fee21e51df02e1c2b3a4c24",
    ),
    (
        ".cursor/rules/justin-guidebook.mdc",
        "f3899110887166721ffef4726983c4a6147efa817e5ec858fa4f0054aef3017a",
    ),
];

const SHARED_ADOPTION_CHANGES: &[&str] = &[
    ".ai-guidebook/GUIDEBOOK.md",
    ".ai-guidebook/policy-lock.json",
    "AGENTS.md",
    ".gitignore",
    "GEMINI.md",
    ".github/copilot-instructions.md",
    ".cursor/rules/justin-guidebook.mdc",
];

#[derive(Clone, Copy)]
enum PolicyFault {
    InvalidDate,
    InvalidVersion,
    SourceTraversal,
    MissingSource,
    AuditModuleOmission,
    DuplicateRuleId,
    MandatoryAuditRuleOmission,
}

impl PolicyFault {
    fn name(self) -> &'static str {
        match self {
            Self::InvalidDate => "invalid-date",
            Self::InvalidVersion => "invalid-version",
            Self::SourceTraversal => "source-traversal",
            Self::MissingSource => "missing-source",
            Self::AuditModuleOmission => "audit-module-omission",
            Self::DuplicateRuleId => "duplicate-rule-id",
            Self::MandatoryAuditRuleOmission => "mandatory-audit-rule-omission",
        }
    }
}

#[test]
#[ignore = "runs the pinned historical Python CLI; executed in the dedicated cross-platform CI job"]
fn historical_python_and_current_rust_cli_match_independent_goldens() {
    let temp = fixture_tempdir();
    let unrelated = temp.path().join("unrelated working directory");
    fs::create_dir_all(&unrelated).unwrap();

    let python_root = clone_python_reference(&temp, "python-golden");
    let rust_root = copy_current_checkout(&temp, "rust-golden");
    let python_non_generated_before_build =
        snapshot_tree_except(&python_root, PYTHON_ARTIFACT_GOLDENS);
    let rust_non_generated_before_build = snapshot_tree_except(&rust_root, RUST_ARTIFACT_GOLDENS);

    let python_build = run_python(&python_root, &unrelated, &["build"]);
    assert_eq!(
        exit_code(&python_build),
        0,
        "Python build failed: {}",
        stderr(&python_build)
    );
    assert_eq!(stdout(&python_build), "Generated 9 files\n");
    assert!(
        python_build.stderr.is_empty(),
        "unexpected Python build warning: {}",
        stderr(&python_build)
    );
    assert_artifact_goldens(&python_root, PYTHON_ARTIFACT_GOLDENS);
    assert_eq!(
        snapshot_tree_except(&python_root, PYTHON_ARTIFACT_GOLDENS),
        python_non_generated_before_build,
        "Python build changed files outside its generated artifact set"
    );

    let python_lock = read_json(&python_root.join("guidebook-lock.json"));
    assert_eq!(python_lock["policy_version"], "1.0.0");
    assert_eq!(
        python_lock["source_sha256"],
        "84d0839bef5454e505f4babd0d7b588f0eb000148d720bc385d0c2472eb4c9ac"
    );
    assert_eq!(
        python_lock["guidebook_sha256"],
        "7e7ea0e5fd53fb6d8e89bf8c7a72b915dc3c555d2af9c949412d75b2aadcf887"
    );
    assert_eq!(python_lock["rule_count"], 125);
    let python_guide = fs::read(python_root.join("GUIDEBOOK.md")).unwrap();
    assert_eq!(
        sha256_hex(&python_guide),
        python_lock["guidebook_sha256"].as_str().unwrap()
    );

    let python_before_check = snapshot_tree(&python_root);
    let python_check = run_python(&python_root, &unrelated, &["check"]);
    assert_eq!(
        exit_code(&python_check),
        0,
        "Python check failed: {}",
        stderr(&python_check)
    );
    assert!(
        python_check.stderr.is_empty(),
        "unexpected Python check warning: {}",
        stderr(&python_check)
    );
    let python_report = json_stdout(&python_check);
    assert_eq!(python_report["status"], "passed");
    assert_eq!(python_report["rules"], 125);
    assert_eq!(
        python_report["guidebook_sha256"],
        python_lock["guidebook_sha256"]
    );
    assert_eq!(
        snapshot_tree(&python_root),
        python_before_check,
        "Python check must not change generated files"
    );

    let python_after_first_build = snapshot_tree(&python_root);
    let python_repeat = run_python(&python_root, &unrelated, &["build"]);
    assert_eq!(
        exit_code(&python_repeat),
        0,
        "repeat Python build failed: {}",
        stderr(&python_repeat)
    );
    assert_eq!(stdout(&python_repeat), "Generated 9 files\n");
    assert_eq!(
        snapshot_tree(&python_root),
        python_after_first_build,
        "the pinned Python build must be byte-identical on repeat"
    );

    let rust_build = run_rust(&rust_root, &unrelated, &["build"]);
    assert_eq!(
        exit_code(&rust_build),
        0,
        "Rust build failed: {}",
        stderr(&rust_build)
    );
    assert_eq!(
        stdout(&rust_build),
        "Generated 9 full-guide, register, and adapter artifacts.\n"
    );
    assert!(
        rust_build.stderr.is_empty(),
        "unexpected Rust build warning: {}",
        stderr(&rust_build)
    );
    assert_artifact_goldens(&rust_root, RUST_ARTIFACT_GOLDENS);
    assert_eq!(
        snapshot_tree_except(&rust_root, RUST_ARTIFACT_GOLDENS),
        rust_non_generated_before_build,
        "Rust build changed files outside its generated artifact set"
    );

    let rust_lock = read_json(&rust_root.join("guidebook-lock.json"));
    assert_eq!(rust_lock["policy_version"], "1.2.5");
    assert_eq!(
        rust_lock["source_sha256"],
        "397a936a96721588934ee1a28feeb59f5a6cfd979f99f3df726e51700198182e"
    );
    assert_eq!(
        rust_lock["guidebook_sha256"],
        "4123ddabf48b5efaa6f5fc4a138afd841094efe09b84fc74edcd1287a286388c"
    );
    assert_eq!(rust_lock["rule_count"], 161);
    assert_eq!(rust_lock["verification_rule_count"], 46);
    let rust_guide = fs::read(rust_root.join("GUIDEBOOK.md")).unwrap();
    assert_eq!(
        sha256_hex(&rust_guide),
        rust_lock["guidebook_sha256"].as_str().unwrap()
    );

    let rust_before_check = snapshot_tree(&rust_root);
    let rust_check = run_rust(&rust_root, &unrelated, &["check"]);
    assert_eq!(
        exit_code(&rust_check),
        0,
        "Rust check failed: {}",
        stderr(&rust_check)
    );
    assert!(
        rust_check.stderr.is_empty(),
        "unexpected Rust check warning: {}",
        stderr(&rust_check)
    );
    assert_eq!(
        stdout(&rust_check),
        "Repository structural checks passed (161 standing rules + 46 verification rules).\n"
    );
    assert_eq!(
        snapshot_tree(&rust_root),
        rust_before_check,
        "Rust check must not change generated files"
    );

    let rust_after_first_build = snapshot_tree(&rust_root);
    let rust_repeat = run_rust(&rust_root, &unrelated, &["build"]);
    assert_eq!(
        exit_code(&rust_repeat),
        0,
        "repeat Rust build failed: {}",
        stderr(&rust_repeat)
    );
    assert_eq!(
        stdout(&rust_repeat),
        "Generated 9 full-guide, register, and adapter artifacts.\n"
    );
    assert_eq!(
        snapshot_tree(&rust_root),
        rust_after_first_build,
        "the current Rust build must be byte-identical on repeat"
    );
    assert_empty_directory(&unrelated);
}

#[test]
#[ignore = "runs the pinned historical Python CLI; executed in the dedicated cross-platform CI job"]
fn cli_help_lists_commands_and_options_without_writes() {
    let temp = fixture_tempdir();
    let unrelated = temp.path().join("unrelated working directory");
    fs::create_dir_all(&unrelated).unwrap();

    let python_root = clone_python_reference(&temp, "python-help");
    let rust_root = copy_current_checkout(&temp, "rust-help");
    let python_before = snapshot_tree(&python_root);
    let rust_before = snapshot_tree(&rust_root);

    let python_help = run_python(&python_root, &unrelated, &["--help"]);
    assert_eq!(
        exit_code(&python_help),
        0,
        "Python help failed: {}",
        stderr(&python_help)
    );
    assert!(
        python_help.stderr.is_empty(),
        "unexpected Python help warning: {}",
        stderr(&python_help)
    );
    let python_text = stdout(&python_help);
    for expected in ["usage:", "build", "check", "adopt"] {
        assert!(
            python_text.contains(expected),
            "Python help omitted {expected:?}: {python_text}"
        );
    }
    assert_eq!(
        snapshot_tree(&python_root),
        python_before,
        "Python help changed its checkout"
    );

    let python_adopt_help = run_python(&python_root, &unrelated, &["adopt", "--help"]);
    assert_eq!(
        exit_code(&python_adopt_help),
        0,
        "Python adopt help failed: {}",
        stderr(&python_adopt_help)
    );
    assert!(
        python_adopt_help.stderr.is_empty(),
        "unexpected Python adopt-help warning: {}",
        stderr(&python_adopt_help)
    );
    let python_adopt_text = stdout(&python_adopt_help);
    for expected in ["usage:", "adopt", "target", "--apply"] {
        assert!(
            python_adopt_text.contains(expected),
            "Python adopt help omitted {expected:?}: {python_adopt_text}"
        );
    }
    assert_eq!(
        snapshot_tree(&python_root),
        python_before,
        "Python adopt help changed its checkout"
    );

    let rust_help = run_rust(&rust_root, &unrelated, &["--help"]);
    assert_eq!(
        exit_code(&rust_help),
        0,
        "Rust help failed: {}",
        stderr(&rust_help)
    );
    assert!(
        rust_help.stderr.is_empty(),
        "unexpected Rust help warning: {}",
        stderr(&rust_help)
    );
    let rust_text = stdout(&rust_help);
    for expected in [
        "build",
        "check",
        "render",
        "refresh-manifest",
        "new-audit",
        "adopt",
        "--root PATH",
        "--apply",
        "--recover",
    ] {
        assert!(
            rust_text.contains(expected),
            "Rust help omitted {expected:?}: {rust_text}"
        );
    }
    assert_eq!(
        snapshot_tree(&rust_root),
        rust_before,
        "Rust help changed its checkout"
    );
    assert_empty_directory(&unrelated);
}

#[test]
#[ignore = "runs the pinned historical Python CLI; executed in the dedicated cross-platform CI job"]
fn both_clis_refuse_invalid_policy_or_source_faults_without_changing_generated_files() {
    let temp = fixture_tempdir();
    let unrelated = temp.path().join("unrelated working directory");
    fs::create_dir_all(&unrelated).unwrap();

    for fault in [
        PolicyFault::InvalidDate,
        PolicyFault::InvalidVersion,
        PolicyFault::SourceTraversal,
        PolicyFault::MissingSource,
        PolicyFault::AuditModuleOmission,
        PolicyFault::DuplicateRuleId,
        PolicyFault::MandatoryAuditRuleOmission,
    ] {
        let python_root = clone_python_reference(&temp, &format!("python-{}", fault.name()));
        let rust_root = copy_current_checkout(&temp, &format!("rust-{}", fault.name()));

        mutate_policy(&python_root, fault);
        mutate_policy(&rust_root, fault);
        let python_before = snapshot_tree(&python_root);
        let rust_before = snapshot_tree(&rust_root);

        let python_result = run_python(&python_root, &unrelated, &["build"]);
        assert_eq!(
            exit_code(&python_result),
            1,
            "pinned Python CLI did not refuse {}: stdout={} stderr={}",
            fault.name(),
            stdout(&python_result),
            stderr(&python_result)
        );
        assert!(
            stderr(&python_result).starts_with("ERROR:"),
            "unexpected Python refusal: {}",
            stderr(&python_result)
        );
        assert!(
            stdout(&python_result).is_empty(),
            "Python refusal wrote to stdout"
        );
        assert_eq!(
            snapshot_tree(&python_root),
            python_before,
            "pinned Python refusal for {} changed generated files",
            fault.name()
        );

        let rust_result = run_rust(&rust_root, &unrelated, &["build"]);
        assert_eq!(
            exit_code(&rust_result),
            1,
            "Rust CLI did not refuse {}: stdout={} stderr={}",
            fault.name(),
            stdout(&rust_result),
            stderr(&rust_result)
        );
        assert!(
            stderr(&rust_result).starts_with("FAIL:"),
            "unexpected Rust refusal: {}",
            stderr(&rust_result)
        );
        assert!(
            stdout(&rust_result).is_empty(),
            "Rust refusal wrote to stdout"
        );
        assert_eq!(
            snapshot_tree(&rust_root),
            rust_before,
            "Rust refusal for {} changed generated files",
            fault.name()
        );
    }
    assert_empty_directory(&unrelated);
}

#[test]
#[ignore = "runs the pinned historical Python CLI; executed in the dedicated cross-platform CI job"]
fn both_clis_refuse_a_missing_generated_guide_without_writes() {
    let temp = fixture_tempdir();
    let unrelated = temp.path().join("unrelated working directory");
    fs::create_dir_all(&unrelated).unwrap();

    let python_root = clone_python_reference(&temp, "python-missing-guide");
    let rust_root = copy_current_checkout(&temp, "rust-missing-guide");
    fs::remove_file(python_root.join("GUIDEBOOK.md")).unwrap();
    fs::remove_file(rust_root.join("GUIDEBOOK.md")).unwrap();
    let python_before = snapshot_tree(&python_root);
    let rust_before = snapshot_tree(&rust_root);

    let python_result = run_python(&python_root, &unrelated, &["check"]);
    assert_eq!(
        exit_code(&python_result),
        1,
        "pinned Python CLI accepted a missing generated guide"
    );
    assert!(stderr(&python_result).starts_with("ERROR:"));
    assert!(python_result.stdout.is_empty());
    assert_eq!(snapshot_tree(&python_root), python_before);

    let rust_result = run_rust(&rust_root, &unrelated, &["check"]);
    assert_eq!(
        exit_code(&rust_result),
        1,
        "current Rust CLI accepted a missing generated guide"
    );
    assert!(stderr(&rust_result).starts_with("FAIL:"));
    assert!(rust_result.stdout.is_empty());
    assert_eq!(snapshot_tree(&rust_root), rust_before);
    assert_empty_directory(&unrelated);
}

#[cfg(windows)]
#[test]
#[ignore = "runs the pinned historical Python CLI; executed in the dedicated Windows CI job"]
fn atomic_replace_failure_preserves_python_and_rust_artifacts_on_windows() {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};

    let temp = fixture_tempdir();
    let unrelated = temp.path().join("unrelated working directory");
    fs::create_dir_all(&unrelated).unwrap();

    let python_root = clone_python_reference(&temp, "python-atomic-replace-failure");
    let rust_root = copy_current_checkout(&temp, "rust-atomic-replace-failure");
    let python_before = snapshot_tree(&python_root);
    let rust_before = snapshot_tree(&rust_root);

    // Permit reads and writes but deny delete sharing, forcing replacement of
    // the existing GUIDEBOOK.md to fail without injecting Python code.
    let share_read_write = 0x0000_0001 | 0x0000_0002;
    let _python_lock = OpenOptions::new()
        .read(true)
        .share_mode(share_read_write)
        .open(python_root.join("GUIDEBOOK.md"))
        .unwrap();
    let _rust_lock = OpenOptions::new()
        .read(true)
        .share_mode(share_read_write)
        .open(rust_root.join("GUIDEBOOK.md"))
        .unwrap();

    let python_result = run_python(&python_root, &unrelated, &["build"]);
    assert_eq!(
        exit_code(&python_result),
        1,
        "pinned Python CLI unexpectedly replaced a locked guide"
    );
    assert!(stderr(&python_result).starts_with("ERROR:"));
    assert_eq!(
        snapshot_tree(&python_root),
        python_before,
        "Python replacement failure changed an artifact or left a temporary"
    );

    let rust_result = run_rust(&rust_root, &unrelated, &["build"]);
    assert_eq!(
        exit_code(&rust_result),
        1,
        "current Rust CLI unexpectedly replaced a locked guide"
    );
    assert!(stderr(&rust_result).starts_with("FAIL:"));
    assert_eq!(
        snapshot_tree(&rust_root),
        rust_before,
        "Rust replacement failure changed an artifact or left a temporary"
    );
    assert_empty_directory(&unrelated);
}

#[test]
#[ignore = "runs the pinned historical Python CLI; executed in the dedicated cross-platform CI job"]
fn adoption_preview_is_read_only_and_preserves_existing_project_files() {
    let temp = fixture_tempdir();
    let unrelated = temp.path().join("unrelated working directory");
    fs::create_dir_all(&unrelated).unwrap();

    let python_root = clone_python_reference(&temp, "python-adoption-preview");
    let rust_root = copy_current_checkout(&temp, "rust-adoption-preview");
    let python_target = temp.path().join("Python target with spaces");
    let rust_target = temp.path().join("Rust target with spaces");
    prepare_target(&python_target);
    prepare_target(&rust_target);

    let python_before = snapshot_tree(&python_target);
    let rust_before = snapshot_tree(&rust_target);

    let python_preview = run_python(
        &python_root,
        &unrelated,
        &[
            "adopt".to_owned(),
            python_target.to_string_lossy().into_owned(),
        ],
    );
    assert_eq!(
        exit_code(&python_preview),
        0,
        "Python preview failed: {}",
        stderr(&python_preview)
    );
    let python_plan = json_stdout(&python_preview);
    assert_eq!(python_plan["mode"], "preview");
    assert_eq!(
        string_set(&python_plan["changed"]),
        expected_adoption_changes()
    );
    assert_eq!(
        string_set(&python_plan["preserved_existing_adapters"]),
        BTreeSet::from(["CLAUDE.md".to_owned()])
    );
    assert_eq!(
        snapshot_tree(&python_target),
        python_before,
        "Python preview wrote to the target"
    );

    let rust_preview = run_rust(
        &rust_root,
        &unrelated,
        &[
            "adopt".to_owned(),
            rust_target.to_string_lossy().into_owned(),
        ],
    );
    assert_eq!(
        exit_code(&rust_preview),
        0,
        "Rust preview failed: {}",
        stderr(&rust_preview)
    );
    let rust_plan = json_stdout(&rust_preview);
    assert_eq!(rust_plan["mode"], "preview");
    assert_eq!(
        string_set(&rust_plan["changed"]),
        expected_adoption_changes()
    );
    assert_eq!(
        string_set(&rust_plan["preserved_existing_adapters"]),
        BTreeSet::from(["CLAUDE.md".to_owned()])
    );
    assert_eq!(
        snapshot_tree(&rust_target),
        rust_before,
        "Rust preview wrote to the target"
    );
    assert!(
        python_preview.stderr.is_empty(),
        "unexpected Python preview warning: {}",
        stderr(&python_preview)
    );
    assert!(
        rust_preview.stderr.is_empty(),
        "unexpected Rust preview warning: {}",
        stderr(&rust_preview)
    );
    assert_empty_directory(&unrelated);
}

#[test]
#[ignore = "runs the pinned historical Python CLI; executed in the dedicated cross-platform CI job"]
fn python_and_rust_apply_respect_platform_policy_and_are_idempotent() {
    let temp = fixture_tempdir();
    let unrelated = temp.path().join("unrelated working directory");
    fs::create_dir_all(&unrelated).unwrap();
    let outside = temp.path().join("outside sentinel.txt");
    fs::write(&outside, b"outside data must remain unchanged").unwrap();

    let python_root = clone_python_reference(&temp, "python-adoption-apply");
    let rust_root = copy_current_checkout(&temp, "rust-adoption-apply");
    let python_target = temp.path().join("Python apply target with spaces");
    let rust_target = temp.path().join("Rust apply target with spaces");
    prepare_target(&python_target);
    prepare_target(&rust_target);

    let python_before = snapshot_tree(&python_target);
    let python_apply = run_python(
        &python_root,
        &unrelated,
        &[
            "adopt".to_owned(),
            python_target.to_string_lossy().into_owned(),
            "--apply".to_owned(),
        ],
    );
    assert_eq!(
        exit_code(&python_apply),
        0,
        "Python apply failed: {}",
        stderr(&python_apply)
    );
    assert!(
        python_apply.stderr.is_empty(),
        "unexpected Python apply warning: {}",
        stderr(&python_apply)
    );
    let applied = json_stdout(&python_apply);
    assert_eq!(applied["mode"], "applied");
    assert_eq!(string_set(&applied["changed"]), expected_adoption_changes());
    assert_eq!(
        string_set(&applied["preserved_existing_adapters"]),
        BTreeSet::from(["CLAUDE.md".to_owned()])
    );

    let python_after_apply = snapshot_tree(&python_target);
    assert_eq!(
        changed_snapshot_entries(&python_before, &python_after_apply),
        expected_adoption_tree_changes(),
        "Python apply changed an unexpected target file or directory"
    );
    for path in ["AGENTS.md", ".gitignore", "CLAUDE.md"] {
        let original = python_before
            .get(path)
            .unwrap_or_else(|| panic!("fixture is missing original {path}"));
        let after = python_after_apply
            .get(path)
            .unwrap_or_else(|| panic!("Python apply removed existing {path}"));
        assert!(
            after.starts_with(original),
            "Python apply did not preserve all existing bytes in {path}"
        );
    }

    let python_guide_path = python_target.join(".ai-guidebook/GUIDEBOOK.md");
    let python_lock = read_json(&python_target.join(".ai-guidebook/policy-lock.json"));
    let python_guide = fs::read(&python_guide_path).unwrap();
    assert_eq!(
        sha256_hex(&python_guide),
        python_lock["guidebook_sha256"].as_str().unwrap()
    );
    assert!(
        fs::read_to_string(python_target.join("AGENTS.md"))
            .unwrap()
            .starts_with("Existing project instructions.\n")
    );
    assert_eq!(
        fs::read(python_target.join("CLAUDE.md")).unwrap(),
        b"Existing adapter content.\n"
    );

    assert_no_git_commit(&python_target);
    let python_repeat = run_python(
        &python_root,
        &unrelated,
        &[
            "adopt".to_owned(),
            python_target.to_string_lossy().into_owned(),
            "--apply".to_owned(),
        ],
    );
    assert_eq!(
        exit_code(&python_repeat),
        0,
        "repeat Python apply failed: {}",
        stderr(&python_repeat)
    );
    assert!(
        python_repeat.stderr.is_empty(),
        "unexpected repeated Python apply warning: {}",
        stderr(&python_repeat)
    );
    let repeated = json_stdout(&python_repeat);
    assert_eq!(repeated["mode"], "applied");
    assert!(string_set(&repeated["changed"]).is_empty());
    assert_eq!(
        snapshot_tree(&python_target),
        python_after_apply,
        "repeat Python apply changed target bytes"
    );
    assert_no_git_commit(&python_target);

    let rust_before = snapshot_tree(&rust_target);
    let rust_apply = run_rust(
        &rust_root,
        &unrelated,
        &[
            "adopt".to_owned(),
            rust_target.to_string_lossy().into_owned(),
            "--apply".to_owned(),
        ],
    );
    #[cfg(target_os = "macos")]
    {
        assert_eq!(
            exit_code(&rust_apply),
            1,
            "macOS Rust apply unexpectedly succeeded"
        );
        assert!(
            stderr(&rust_apply).starts_with("FAIL:"),
            "unexpected Rust refusal: {}",
            stderr(&rust_apply)
        );
        assert!(
            stderr(&rust_apply).to_ascii_lowercase().contains("paused"),
            "Rust apply refusal was not the documented safety pause: {}",
            stderr(&rust_apply)
        );
        assert_eq!(
            snapshot_tree(&rust_target),
            rust_before,
            "paused Rust apply changed the target"
        );
    }
    #[cfg(any(target_os = "linux", windows))]
    {
        assert_eq!(
            exit_code(&rust_apply),
            0,
            "Rust apply failed: {}",
            stderr(&rust_apply)
        );
        assert!(
            rust_apply.stderr.is_empty(),
            "unexpected Rust apply warning: {}",
            stderr(&rust_apply)
        );
        let rust_result = json_stdout(&rust_apply);
        assert_eq!(rust_result["mode"], "applied");
        assert_eq!(
            string_set(&rust_result["changed"]),
            expected_adoption_changes()
        );
        let rust_after_apply = snapshot_tree(&rust_target);
        assert_eq!(
            changed_snapshot_entries(&rust_before, &rust_after_apply),
            expected_adoption_tree_changes(),
            "Rust apply changed an unexpected target file or directory"
        );
        for path in ["AGENTS.md", ".gitignore"] {
            assert!(
                rust_after_apply[path].starts_with(&rust_before[path]),
                "Rust apply did not preserve existing bytes in {path}"
            );
        }
        assert_eq!(
            rust_after_apply["CLAUDE.md"], rust_before["CLAUDE.md"],
            "Rust apply changed the existing adapter"
        );
        let rust_guide = fs::read(rust_target.join(".ai-guidebook/GUIDEBOOK.md")).unwrap();
        let rust_lock = read_json(&rust_target.join(".ai-guidebook/policy-lock.json"));
        assert_eq!(
            sha256_hex(&rust_guide),
            rust_lock["guidebook_sha256"].as_str().unwrap()
        );
        assert_no_git_commit(&rust_target);

        let rust_repeat = run_rust(
            &rust_root,
            &unrelated,
            &[
                "adopt".to_owned(),
                rust_target.to_string_lossy().into_owned(),
                "--apply".to_owned(),
            ],
        );
        assert_eq!(
            exit_code(&rust_repeat),
            0,
            "repeat Rust apply failed: {}",
            stderr(&rust_repeat)
        );
        assert!(
            rust_repeat.stderr.is_empty(),
            "unexpected repeated Rust apply warning: {}",
            stderr(&rust_repeat)
        );
        let repeated = json_stdout(&rust_repeat);
        assert_eq!(repeated["mode"], "applied");
        assert!(string_set(&repeated["changed"]).is_empty());
        assert_eq!(
            snapshot_tree(&rust_target),
            rust_after_apply,
            "repeat Rust apply changed target bytes"
        );
        assert_no_git_commit(&rust_target);
    }
    assert_eq!(
        fs::read(&outside).unwrap(),
        b"outside data must remain unchanged"
    );
    assert_empty_directory(&unrelated);
}

fn fixture_tempdir() -> TempDir {
    Builder::new()
        .prefix("Toe Tickling parity fixtures ")
        .tempdir_in(canonical_temp_parent())
        .expect("create disposable parity fixture directory")
}

fn canonical_temp_parent() -> PathBuf {
    let canonical = env::temp_dir()
        .canonicalize()
        .expect("resolve canonical system temporary directory");

    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};

        let encoded: Vec<u16> = canonical.as_os_str().encode_wide().collect();
        const VERBATIM_PREFIX: [u16; 4] = [92, 92, 63, 92];
        const UNC_PREFIX: [u16; 4] = [85, 78, 67, 92];
        // Short canonical parents use ordinary forms understood by Git for Windows.
        if encoded.len() < 240 && encoded.starts_with(&VERBATIM_PREFIX) {
            let rest = &encoded[VERBATIM_PREFIX.len()..];
            if rest.len() >= 3 && rest[1] == 58 && rest[2] == 92 {
                return PathBuf::from(std::ffi::OsString::from_wide(rest));
            }
            if rest.starts_with(&UNC_PREFIX) {
                let mut normal = Vec::with_capacity(encoded.len() - 2);
                normal.extend_from_slice(&[92, 92]);
                normal.extend_from_slice(&rest[UNC_PREFIX.len()..]);
                return PathBuf::from(std::ffi::OsString::from_wide(&normal));
            }
        }
    }

    canonical
}

fn python_executable() -> std::ffi::OsString {
    env::var_os("TOE_PYTHON_REFERENCE")
        .expect("TOE_PYTHON_REFERENCE must point to the pinned Python 3.15.2 interpreter")
}

fn verify_python_version() {
    let mut command = Command::new(python_executable());
    command
        .args(["-I", "-B", "--version"])
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONHOME");
    isolate_child_environment(&mut command);
    let output = command.output().expect("start pinned Python interpreter");
    assert!(
        output.status.success(),
        "Python --version failed: {}",
        stderr(&output)
    );
    let version = format!("{}{}", stdout(&output), stderr(&output));
    assert_eq!(version.trim(), PYTHON_REFERENCE_VERSION);
}

fn clone_python_reference(parent: &TempDir, name: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let destination = parent.path().join(name);
    let template = parent.path().join("empty Git template");
    fs::create_dir_all(&template).expect("create empty Git template");

    let mut clone = Command::new("git");
    clone
        .args([
            "clone",
            "--quiet",
            "--local",
            "--no-hardlinks",
            "--no-checkout",
        ])
        .arg(format!("--template={}", template.display()))
        .arg(source)
        .arg(&destination);
    isolate_child_environment(&mut clone);
    let clone = clone.output().expect("start local historical source clone");
    assert!(
        clone.status.success(),
        "could not clone the local repository history for the reference CLI: {}",
        stderr(&clone)
    );

    let checkout = git_output(
        &destination,
        &[
            "-c",
            &format!("core.hooksPath={}", template.display()),
            "checkout",
            "--quiet",
            "--detach",
            PYTHON_REFERENCE_COMMIT,
        ],
    );
    assert!(
        checkout.status.success(),
        "historical reference commit is unavailable in the full-history checkout: {}",
        stderr(&checkout)
    );

    let head = git_output(&destination, &["rev-parse", "HEAD"]);
    assert!(
        head.status.success(),
        "cannot identify historical checkout HEAD"
    );
    assert_eq!(stdout(&head).trim(), PYTHON_REFERENCE_COMMIT);

    let script_path = destination.join("scripts/guidebook.py");
    let script = fs::read(&script_path).expect("read pinned historical CLI source");
    assert_eq!(
        sha256_hex(&script),
        PYTHON_REFERENCE_SCRIPT_SHA256,
        "historical Python CLI bytes do not match the reviewed source hash"
    );
    destination
}

fn copy_current_checkout(parent: &TempDir, name: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let destination = parent.path().join(name);
    fs::create_dir_all(&destination).expect("create current Rust fixture checkout");

    let mut listing = Command::new("git");
    listing.args(["ls-files", "-z"]).current_dir(source);
    isolate_child_environment(&mut listing);
    let listing = listing
        .output()
        .expect("list tracked current checkout files");
    assert!(
        listing.status.success(),
        "cannot list tracked current checkout files: {}",
        stderr(&listing)
    );

    for entry in listing
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let relative = Path::new(std::str::from_utf8(entry).expect("tracked path is UTF-8"));
        assert!(
            !relative.is_absolute()
                && relative
                    .components()
                    .all(|component| matches!(component, Component::Normal(_))),
            "tracked path must remain relative"
        );
        assert!(
            !relative.components().any(|component| {
                matches!(
                    component.as_os_str().to_str(),
                    Some(".git" | "target" | "private" | ".private" | ".local")
                )
            }),
            "parity fixtures refuse private, metadata, and build-output paths"
        );

        let source_path = source.join(relative);
        let metadata = fs::symlink_metadata(&source_path).unwrap_or_else(|error| {
            panic!(
                "cannot inspect tracked file {}: {error}",
                relative.display()
            )
        });
        assert!(
            !metadata.file_type().is_symlink(),
            "parity fixtures refuse tracked symlinks: {}",
            relative.display()
        );
        assert!(
            metadata.is_file(),
            "tracked path is not a regular file: {}",
            relative.display()
        );

        let destination_path = destination.join(relative);
        fs::create_dir_all(destination_path.parent().expect("tracked file has parent"))
            .expect("create fixture parent directory");
        fs::copy(source_path, destination_path).expect("copy tracked fixture file");
    }

    destination
}

fn git_output(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new("git");
    command.args(args).current_dir(root);
    isolate_child_environment(&mut command);
    command.output().expect("start Git fixture command")
}

fn prepare_target(target: &Path) {
    fs::create_dir_all(target).expect("create synthetic Git target");
    let template = target
        .parent()
        .expect("synthetic target has a parent")
        .join("empty Git template");
    fs::create_dir_all(&template).expect("create empty Git template");
    let mut command = Command::new("git");
    command
        .args(["init", "--quiet"])
        .arg(format!("--template={}", template.display()))
        .arg(target);
    isolate_child_environment(&mut command);
    let init = command
        .output()
        .expect("start Git init for synthetic target");
    assert!(init.status.success(), "Git init failed: {}", stderr(&init));

    fs::write(
        target.join("AGENTS.md"),
        "Existing project instructions.\nNever replace this fixture prefix.\n",
    )
    .expect("write synthetic project instructions");
    fs::write(target.join(".gitignore"), "build/\n*.generated\n")
        .expect("write synthetic ignore rules");
    fs::write(target.join("CLAUDE.md"), "Existing adapter content.\n")
        .expect("write synthetic adapter");
}

fn run_python<S: AsRef<std::ffi::OsStr>>(root: &Path, cwd: &Path, args: &[S]) -> Output {
    verify_python_version();
    let mut command = Command::new(python_executable());
    command
        .args(["-I", "-B"])
        .arg(root.join("scripts/guidebook.py"))
        .args(args)
        .current_dir(cwd)
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONHOME");
    isolate_child_environment(&mut command);
    command.output().expect("run pinned historical Python CLI")
}

fn run_rust<S: AsRef<std::ffi::OsStr>>(root: &Path, cwd: &Path, args: &[S]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_toe-tickling"));
    command.arg("--root").arg(root).args(args).current_dir(cwd);
    isolate_child_environment(&mut command);
    command.output().expect("run current Rust CLI")
}

fn isolate_child_environment(command: &mut Command) {
    for &name in SENSITIVE_CHILD_ENV.iter().chain(GIT_CONTEXT_ENV.iter()) {
        command.env_remove(name);
    }
    let secret_names: Vec<_> = env::vars_os()
        .filter_map(|(name, _)| {
            let uppercase = name.to_string_lossy().to_ascii_uppercase();
            [
                "TOKEN",
                "SECRET",
                "PASSWORD",
                "API_KEY",
                "PRIVATE_KEY",
                "CREDENTIAL",
                "PROXY",
            ]
            .iter()
            .any(|marker| uppercase.contains(marker))
            .then_some(name)
        })
        .collect();
    for name in secret_names {
        command.env_remove(name);
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
}

fn mutate_policy(root: &Path, fault: PolicyFault) {
    let path = root.join("policy.json");
    let mut policy = read_json(&path);
    match fault {
        PolicyFault::InvalidDate => policy["updated"] = Value::String("2026-10-15".to_owned()),
        PolicyFault::InvalidVersion => {
            policy["policy_version"] = Value::String("latest".to_owned())
        }
        PolicyFault::SourceTraversal => policy["sources"]
            .as_array_mut()
            .expect("policy sources are an array")
            .push(Value::String("../outside.md".to_owned())),
        PolicyFault::MissingSource => policy["sources"]
            .as_array_mut()
            .expect("policy sources are an array")
            .push(Value::String("rules/99-missing-fixture.md".to_owned())),
        PolicyFault::AuditModuleOmission => policy["sources"]
            .as_array_mut()
            .expect("policy sources are an array")
            .retain(|source| source.as_str() != Some("rules/60-instruction-audits.md")),
        PolicyFault::DuplicateRuleId => {
            let path = root.join("rules/00-authority.md");
            let source = fs::read_to_string(&path).expect("read synthetic authority source");
            fs::write(
                path,
                format!("{source}\n- **AUTH-001** Duplicate synthetic rule ID.\n"),
            )
            .expect("write duplicate synthetic rule ID");
        }
        PolicyFault::MandatoryAuditRuleOmission => {
            let path = root.join("rules/60-instruction-audits.md");
            let source = fs::read_to_string(&path).expect("read synthetic audit rules");
            let marker = "- **AUD-001**";
            assert_eq!(source.matches(marker).count(), 1);
            fs::write(path, source.replacen(marker, "- **AUD-099**", 1))
                .expect("remove the required audit rule from the synthetic fixture");
        }
    }
    let text = serde_json::to_vec_pretty(&policy).expect("serialize synthetic policy fault");
    fs::write(path, [text.as_slice(), b"\n"].concat()).expect("write synthetic invalid policy");
}

fn expected_adoption_changes() -> BTreeSet<String> {
    SHARED_ADOPTION_CHANGES
        .iter()
        .map(|path| (*path).to_owned())
        .collect()
}

fn expected_adoption_tree_changes() -> BTreeSet<String> {
    let mut expected = expected_adoption_changes();
    expected.extend(
        [".ai-guidebook/", ".github/", ".cursor/", ".cursor/rules/"].map(|path| path.to_owned()),
    );
    expected
}

fn changed_snapshot_entries(
    before: &BTreeMap<String, Vec<u8>>,
    after: &BTreeMap<String, Vec<u8>>,
) -> BTreeSet<String> {
    before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .cloned()
        .collect()
}

fn assert_artifact_goldens(root: &Path, expected: &[(&str, &str)]) {
    for &(relative, expected_sha256) in expected {
        let path = root.join(relative);
        let bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!("cannot read generated artifact {}: {error}", path.display())
        });
        assert_eq!(
            sha256_hex(&bytes),
            expected_sha256,
            "generated artifact digest mismatch for {}",
            path.display()
        );
    }
}

fn snapshot_tree_except(root: &Path, ignored_paths: &[(&str, &str)]) -> BTreeMap<String, Vec<u8>> {
    let ignored: BTreeSet<_> = ignored_paths
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .collect();
    snapshot_tree(root)
        .into_iter()
        .filter(|(path, _)| !ignored.contains(path))
        .collect()
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .expect("CLI result field is an array")
        .iter()
        .map(|item| {
            item.as_str()
                .expect("CLI result item is a string")
                .to_owned()
        })
        .collect()
}

fn json_stdout(output: &Output) -> Value {
    serde_json::from_str(stdout(output).trim()).expect("CLI stdout contains JSON")
}

fn read_json(path: &Path) -> Value {
    let bytes = fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn snapshot_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut snapshot = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).expect("read fixture directory") {
            let entry = entry.expect("read synthetic target entry");
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .expect("snapshot entry stays under target")
                .to_path_buf();
            let file_type = entry.file_type().expect("inspect synthetic target entry");
            assert!(
                !file_type.is_symlink(),
                "synthetic target unexpectedly contains a symlink"
            );
            if file_type.is_dir() {
                let key = format!("{}/", relative.to_string_lossy().replace('\\', "/"));
                snapshot.insert(key, Vec::new());
                pending.push(path);
            } else {
                assert!(file_type.is_file(), "synthetic target contains a non-file");
                let key = relative.to_string_lossy().replace('\\', "/");
                snapshot.insert(key, fs::read(path).expect("read synthetic target file"));
            }
        }
    }
    snapshot
}

fn assert_no_git_commit(target: &Path) {
    let head = git_output(target, &["rev-parse", "--verify", "HEAD"]);
    assert!(
        !head.status.success(),
        "adoption unexpectedly created a Git commit"
    );
}

fn assert_empty_directory(path: &Path) {
    let mut entries =
        fs::read_dir(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    assert!(
        entries.next().is_none(),
        "CLI wrote into unrelated working directory {}",
        path.display()
    );
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn exit_code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}
