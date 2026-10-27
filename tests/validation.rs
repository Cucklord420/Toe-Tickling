use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::Command,
    time::{Duration, SystemTime},
};
use tempfile::TempDir;
use time::OffsetDateTime;
use toe_tickling::*;

fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}
fn fixture() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in READING_FILES.iter().chain(guide::SOURCES.iter()) {
        let target = dir.path().join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(path), target).unwrap();
    }
    for file in [
        ".gitignore",
        "LICENSE",
        "LICENSE-SCOPE.md",
        "CHANGELOG.md",
        ".github/workflows/validate.yml",
        "docs/VALIDATION.md",
        "docs/archive/INITIAL_INSTRUCTION_AUDIT.md",
        "docs/archive/INITIAL_VALIDATION.md",
    ] {
        fs::create_dir_all(dir.path().join(file).parent().unwrap()).unwrap();
        fs::copy(source.join(file), dir.path().join(file)).unwrap();
    }
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/fixture.rs"),
        "pub const VALUE: u8 = 1;\n",
    )
    .unwrap();
    let policy = load_policy(dir.path()).unwrap();
    fs::write(dir.path().join("RULEBOOK.md"), render(&policy)).unwrap();
    guide::build(dir.path()).unwrap();
    write_json(
        &dir.path().join("policy/reading-manifest.json"),
        &current_manifest(dir.path()).unwrap(),
    )
    .unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "Fictional fixture",
        ],
    ] {
        let output = clean_git(dir.path(), &args);
        assert!(
            output.status.success(),
            "Git fixture operation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    dir
}
fn complete(root: &Path) -> Audit {
    let mut audit = new_audit(root, "Validate a fictional isolated fixture".into()).unwrap();
    audit.recorded_at = "2026-10-15T12:00:00Z".into();
    audit.outcome = "complete".into();
    for receipt in &mut audit.reading {
        receipt.note = "Fictional application note for structural test only".into();
    }
    audit.evidence = BTreeMap::from([(
        "fixture".into(),
        "Fictional observation, not a real task-compliance claim".into(),
    )]);
    for check in audit.checks.iter_mut().chain(audit.acceptance.iter_mut()) {
        check.status = "pass".into();
        check.reason = "Fictional verified condition for structural fixture".into();
        check.evidence = vec!["fixture".into()];
    }
    audit
}
#[test]
fn rust_policy_requires_relevant_official_guidance_and_evidence() {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    let rust_rule = policy
        .rules
        .iter()
        .find(|rule| rule.id == "ENG-07" && rule.status == "active")
        .expect("Required Rust guidance verification rule is missing or inactive");
    assert!(
        rust_rule
            .requirement
            .contains("https://doc.rust-lang.org/stable/book/")
    );
    assert!(
        rust_rule
            .requirement
            .contains("https://doc.rust-lang.org/releases.html")
    );
    assert!(
        rust_rule
            .requirement
            .contains("project's pinned Rust compiler")
    );
    assert!(rust_rule.requirement.contains("original-language parity"));
    assert!(
        rust_rule
            .requirement
            .contains("Reading the entire Reference and every nested page is not required")
    );
    assert!(rust_rule.verification.contains("actually read"));
    assert!(rust_rule.requirement.contains("rustfmt"));
    assert!(
        rust_rule
            .verification
            .contains("Rust formatting/lint/testing")
    );
    let standing = fs::read_to_string(dir.path().join("rules/30-engineering.md")).unwrap();
    assert!(standing.contains("- **ENG-020**"));
    assert!(standing.contains("https://doc.rust-lang.org/stable/book/"));
    assert!(standing.contains("https://doc.rust-lang.org/releases.html"));
    assert!(standing.contains("an exhaustive reading of every Reference page is NOT required"));
}

fn clean_git(root: &Path, args: &[&str]) -> std::process::Output {
    let mut command = Command::new("git");
    command.args(args).current_dir(root);
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(key);
        }
    }
    command.output().unwrap()
}

fn normalize_fsmonitor_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    let path = if path
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("//?/UNC/"))
    {
        format!("//{}", &path[8..])
    } else if path
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("//?/"))
    {
        path[4..].to_owned()
    } else {
        path
    };

    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/' {
        format!(
            "/{}/{}",
            (bytes[0] as char).to_ascii_lowercase(),
            &path[3..]
        )
    } else {
        path
    }
}

fn fsmonitor_hook_command(path: &Path) -> String {
    let path = normalize_fsmonitor_path(&path.to_string_lossy());
    format!("sh '{}'", path.replace('\'', "'\\''"))
}

#[test]
fn fsmonitor_hook_command_normalizes_windows_drive_unc_and_extended_paths() {
    for (path, expected) in [
        (r"C:\Task Dir\hook.sh", "sh '/c/Task Dir/hook.sh'"),
        (
            r"\\server\share\Task Dir\hook.sh",
            "sh '//server/share/Task Dir/hook.sh'",
        ),
        (r"\\?\C:\Task Dir\hook.sh", "sh '/c/Task Dir/hook.sh'"),
        (
            r"\\?\UNC\server\share\Task Dir\hook.sh",
            "sh '//server/share/Task Dir/hook.sh'",
        ),
    ] {
        assert_eq!(fsmonitor_hook_command(Path::new(path)), expected);
    }
}

fn add_linked_worktree(repository: &Path, target: &Path) {
    let target = target.to_string_lossy().into_owned();
    let output = clean_git(
        repository,
        &["worktree", "add", "--detach", &target, "HEAD"],
    );
    assert!(
        output.status.success(),
        "Linked-worktree fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(Path::new(&target).join(".git").is_file());
}

fn commit_fixture(root: &Path, name: &str) {
    for args in [
        vec!["init", "-q"],
        vec!["add", "fixture.txt"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            name,
        ],
    ] {
        if args.first() == Some(&"add") {
            fs::write(root.join("fixture.txt"), "Fictional fixture\n").unwrap();
        }
        let output = clean_git(root, &args);
        assert!(
            output.status.success(),
            "Git fixture operation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn baseline_and_declared_receipt_pass() {
    let dir = fixture();
    let policy = check_repo(dir.path(), now().date()).unwrap();
    let audit = complete(dir.path());
    assert_eq!(
        audit.checks.len(),
        toe_tickling::guide::module_ids(dir.path()).unwrap().len()
            + policy
                .rules
                .iter()
                .filter(|rule| rule.status == "active")
                .count()
    );
    validate_audit(dir.path(), &policy, &audit, now()).unwrap();
}

fn assert_source_change_rejects_audits(path: &str, staged: bool) {
    let dir = fixture();
    assert!(
        clean_git(dir.path(), &["config", "status.showUntrackedFiles", "no"])
            .status
            .success()
    );
    let policy = load_policy(dir.path()).unwrap();
    let audit = complete(dir.path());
    validate_audit(dir.path(), &policy, &audit, now()).unwrap();
    let manifest = current_manifest(dir.path()).unwrap();
    let changed_content = "// FICTIONAL_PRIVATE_SOURCE_VALUE\npub const VALUE: u8 = 2;\n";
    fs::write(dir.path().join(path), changed_content).unwrap();
    if staged {
        assert!(clean_git(dir.path(), &["add", "--", path]).status.success());
    }
    let head = clean_git(dir.path(), &["rev-parse", "HEAD"]);
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        audit.repository_revision
    );
    assert_eq!(
        current_manifest(dir.path()).unwrap().documents,
        manifest.documents
    );

    for error in [
        new_audit(dir.path(), "Fictional dirty-source task".into()).unwrap_err(),
        validate_audit(dir.path(), &policy, &audit, now()).unwrap_err(),
    ] {
        assert!(
            error.contains("clean"),
            "Unexpected audit rejection: {error}"
        );
        assert!(!error.contains(path));
        assert!(!error.contains(Path::new(path).file_name().unwrap().to_str().unwrap()));
        assert!(!error.contains(dir.path().to_string_lossy().as_ref()));
        for line in changed_content.lines() {
            assert!(!error.contains(line));
        }
        for status in [" M ", "M  ", "?? "] {
            assert!(!error.contains(status));
        }
    }
}

#[test]
fn staged_source_changes_reject_creation_and_invalidate_existing_receipts() {
    assert_source_change_rejects_audits("src/fixture.rs", true);
}

#[test]
fn unstaged_source_changes_reject_creation_and_invalidate_existing_receipts() {
    assert_source_change_rejects_audits("src/fixture.rs", false);
}

#[test]
fn untracked_source_changes_reject_creation_and_invalidate_existing_receipts() {
    assert_source_change_rejects_audits("src/private-untracked-fixture.rs", false);
}

#[test]
fn same_size_tracked_changes_reject_audits_when_git_stat_cache_matches() {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    let audit = complete(dir.path());
    let tracked = dir.path().join("src/fixture.rs");
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&tracked)
        .unwrap();
    let cached_modified = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    file.set_times(fs::FileTimes::new().set_modified(cached_modified))
        .unwrap();
    assert!(
        clean_git(dir.path(), &["config", "core.trustctime", "false"])
            .status
            .success()
    );
    let refreshed = clean_git(dir.path(), &["update-index", "--refresh"]);
    assert!(
        refreshed.status.success(),
        "Index refresh failed: {}",
        String::from_utf8_lossy(&refreshed.stderr)
    );
    let cached_modified = fs::metadata(&tracked).unwrap().modified().unwrap();

    fs::write(&tracked, "pub const VALUE: u8 = 2;\n").unwrap();
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&tracked)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(cached_modified))
        .unwrap();

    let ordinary_status = clean_git(
        dir.path(),
        &[
            "-c",
            "core.trustctime=false",
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
        ],
    );
    assert!(ordinary_status.status.success());
    assert!(
        ordinary_status.stdout.is_empty(),
        "Fixture did not reproduce the Git stat-cache blind spot: {}",
        String::from_utf8_lossy(&ordinary_status.stdout)
    );

    for error in [
        new_audit(dir.path(), "Fictional stat-cache task".into()).unwrap_err(),
        validate_audit(dir.path(), &policy, &audit, now()).unwrap_err(),
    ] {
        assert!(error.contains("clean"), "Unexpected audit error: {error}");
        assert!(!error.contains("src/fixture.rs"));
        assert!(!error.contains("pub const VALUE"));
        assert!(!error.contains(dir.path().to_string_lossy().as_ref()));
    }
}

fn assert_hidden_source_change_rejects_audits(index_flag: &str) {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    let audit = complete(dir.path());
    let path = "src/fixture.rs";
    let changed_content = "// FICTIONAL_PRIVATE_SOURCE_VALUE\npub const VALUE: u8 = 2;\n";
    assert!(
        clean_git(dir.path(), &["update-index", index_flag, "--", path])
            .status
            .success()
    );
    fs::write(dir.path().join(path), changed_content).unwrap();

    // Git's ordinary status hides this tracked change when either index flag is set.
    let status = clean_git(
        dir.path(),
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    );
    assert!(status.status.success());
    assert!(status.stdout.is_empty());

    for error in [
        new_audit(dir.path(), "Fictional hidden-source task".into()).unwrap_err(),
        validate_audit(dir.path(), &policy, &audit, now()).unwrap_err(),
    ] {
        assert!(
            error.contains("complete Git checkout"),
            "Unexpected audit rejection: {error}"
        );
        assert!(!error.contains(path));
        assert!(!error.contains("FICTIONAL_PRIVATE_SOURCE_VALUE"));
        assert!(!error.contains(dir.path().to_string_lossy().as_ref()));
    }
}

#[test]
fn active_clean_filter_is_rejected_before_content_hashing() {
    let dir = fixture();
    fs::write(
        dir.path().join(".gitattributes"),
        "*.rs filter=audit-probe\n",
    )
    .unwrap();
    assert!(
        clean_git(dir.path(), &["add", "--", ".gitattributes"])
            .status
            .success()
    );
    assert!(
        clean_git(dir.path(), &["commit", "-qm", "Fictional filter fixture"])
            .status
            .success()
    );
    assert!(
        clean_git(dir.path(), &["config", "filter.audit-probe.clean", "false"])
            .status
            .success()
    );

    let error = new_audit(dir.path(), "Fictional filter task".into()).unwrap_err();
    assert!(
        error.contains("complete Git checkout"),
        "Active external filter should fail closed before hashing: {error}"
    );
    assert!(!error.contains("src/fixture.rs"));
    assert!(!error.contains(dir.path().to_string_lossy().as_ref()));
}

#[test]
fn assume_unchanged_source_changes_reject_creation_and_invalidate_receipts() {
    assert_hidden_source_change_rejects_audits("--assume-unchanged");
}

#[test]
fn skip_worktree_source_changes_reject_creation_and_invalidate_receipts() {
    assert_hidden_source_change_rejects_audits("--skip-worktree");
}

#[test]
fn fsmonitor_valid_source_changes_reject_creation_and_invalidate_receipts() {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    let audit = complete(dir.path());
    let path = "src/fixture.rs";
    let monitor = tempfile::tempdir().unwrap();
    let hook = monitor.path().join("no-changes.sh");
    fs::write(&hook, b"printf 'noop\\000'\n").unwrap();
    let hook_command = fsmonitor_hook_command(&hook);
    assert!(
        clean_git(dir.path(), &["config", "core.fsmonitorHookVersion", "2"])
            .status
            .success()
    );
    assert!(
        clean_git(dir.path(), &["config", "core.fsmonitor", &hook_command])
            .status
            .success()
    );
    assert!(
        clean_git(dir.path(), &["update-index", "--fsmonitor"])
            .status
            .success()
    );
    assert!(
        clean_git(
            dir.path(),
            &["update-index", "--fsmonitor-valid", "--", path]
        )
        .status
        .success()
    );
    let flag = clean_git(dir.path(), &["ls-files", "-f", "--", path]);
    assert!(flag.status.success());
    assert!(String::from_utf8_lossy(&flag.stdout).starts_with("h "));

    let changed_content = "// FICTIONAL_PRIVATE_SOURCE_VALUE\npub const VALUE: u8 = 2;\n";
    fs::write(dir.path().join(path), changed_content).unwrap();
    let ordinary_status = clean_git(
        dir.path(),
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    );
    assert!(ordinary_status.status.success());
    assert!(ordinary_status.stdout.is_empty());

    for error in [
        new_audit(dir.path(), "Fictional fsmonitor-hidden task".into()).unwrap_err(),
        validate_audit(dir.path(), &policy, &audit, now()).unwrap_err(),
    ] {
        assert!(
            error.contains("clean"),
            "Unexpected audit rejection: {error}"
        );
        assert!(!error.contains(path));
        assert!(!error.contains("FICTIONAL_PRIVATE_SOURCE_VALUE"));
        assert!(!error.contains(dir.path().to_string_lossy().as_ref()));
    }
}

#[test]
fn clean_source_tree_allows_audits_with_ignored_private_and_build_outputs() {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    fs::write(dir.path().join(".env"), "FICTIONAL_IGNORED_PRIVATE_VALUE\n").unwrap();
    fs::create_dir_all(dir.path().join("target")).unwrap();
    fs::write(
        dir.path().join("target/fixture-output"),
        "Fictional output\n",
    )
    .unwrap();
    let status = clean_git(
        dir.path(),
        &["status", "--porcelain=v1", "--untracked-files=all"],
    );
    assert!(status.status.success());
    assert!(status.stdout.is_empty());
    let audit = complete(dir.path());
    assert_eq!(audit.schema_version, 1);
    validate_audit(dir.path(), &policy, &audit, now()).unwrap();
}

#[test]
fn template_is_never_completion() {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    let audit = new_audit(dir.path(), "Fictional unfinished task".into()).unwrap();
    assert!(validate_audit(dir.path(), &policy, &audit, now()).is_err());
}
#[test]
fn changed_guidance_invalidates_manifest_and_receipt() {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    let mut audit = complete(dir.path());
    fs::write(dir.path().join("AGENTS.md"), "# Changed instructions\n").unwrap();
    assert!(
        check_repo(dir.path(), now().date())
            .unwrap_err()
            .contains("manifest")
    );
    // Keep the receipt's HEAD current so this isolates the reading-hash check.
    assert!(
        clean_git(dir.path(), &["add", "AGENTS.md"])
            .status
            .success()
    );
    assert!(
        clean_git(
            dir.path(),
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "Fictional changed guidance",
            ],
        )
        .status
        .success()
    );
    let head = clean_git(dir.path(), &["rev-parse", "HEAD"]);
    assert!(head.status.success());
    audit.repository_revision = String::from_utf8(head.stdout).unwrap().trim().to_owned();
    assert!(
        validate_audit(dir.path(), &policy, &audit, now())
            .unwrap_err()
            .contains("reading")
    );
}
#[test]
fn missing_intake_document_fails() {
    let dir = fixture();
    fs::remove_file(dir.path().join("docs/ADOPTION.md")).unwrap();
    assert!(check_repo(dir.path(), now().date()).is_err());
}
#[test]
fn canonical_change_requires_render() {
    let dir = fixture();
    let mut policy = load_policy(dir.path()).unwrap();
    policy.rules[0]
        .requirement
        .push_str(" Additional requirement.");
    write_json(&dir.path().join("policy/rules.json"), &policy).unwrap();
    assert!(
        check_repo(dir.path(), now().date())
            .unwrap_err()
            .contains("RULEBOOK")
    );
}
#[test]
fn duplicate_unknown_missing_rule_checks_fail() {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    let mut duplicate = complete(dir.path());
    duplicate.checks.push(duplicate.checks[0].clone());
    assert!(validate_audit(dir.path(), &policy, &duplicate, now()).is_err());
    let mut unknown = complete(dir.path());
    unknown.checks[0].id = "FAKE-01".into();
    assert!(validate_audit(dir.path(), &policy, &unknown, now()).is_err());
    let mut missing = complete(dir.path());
    missing.checks.pop();
    assert!(validate_audit(dir.path(), &policy, &missing, now()).is_err());
}
#[test]
fn missing_or_fictional_placeholder_evidence_fails() {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    let mut audit = complete(dir.path());
    audit.evidence.clear();
    assert!(validate_audit(dir.path(), &policy, &audit, now()).is_err());
    let mut audit = complete(dir.path());
    audit.checks[0].evidence.clear();
    assert!(validate_audit(dir.path(), &policy, &audit, now()).is_err());
    let mut audit = complete(dir.path());
    audit.reading[0].note = "TODO fill this in eventually".into();
    assert!(validate_audit(dir.path(), &policy, &audit, now()).is_err());
}
#[test]
fn blocked_failed_or_waived_acceptance_cannot_pass() {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    for status in ["blocked", "fail", "not-applicable"] {
        let mut audit = complete(dir.path());
        audit.acceptance[0].status = status.into();
        assert!(validate_audit(dir.path(), &policy, &audit, now()).is_err());
    }
    let mut audit = complete(dir.path());
    audit.acceptance.clear();
    assert!(validate_audit(dir.path(), &policy, &audit, now()).is_err());
}
#[test]
fn not_applicable_requires_reason_and_cannot_hide_global_rules() {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    let scoped = policy
        .rules
        .iter()
        .position(|r| r.scope != "global")
        .unwrap();
    let mut audit = complete(dir.path());
    audit.checks[scoped].status = "not-applicable".into();
    audit.checks[scoped].reason = "This isolated documentation task does not run automation".into();
    validate_audit(dir.path(), &policy, &audit, now()).unwrap();
    audit.checks[scoped].reason.clear();
    assert!(validate_audit(dir.path(), &policy, &audit, now()).is_err());
    let mut audit = complete(dir.path());
    audit.checks[0].status = "not-applicable".into();
    assert!(validate_audit(dir.path(), &policy, &audit, now()).is_err());
}
#[test]
fn duplicate_and_incomplete_reading_fail() {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    let mut audit = complete(dir.path());
    audit.reading.push(audit.reading[0].clone());
    assert!(validate_audit(dir.path(), &policy, &audit, now()).is_err());
    let mut audit = complete(dir.path());
    audit.reading.pop();
    assert!(validate_audit(dir.path(), &policy, &audit, now()).is_err());
}
#[test]
fn future_timestamp_and_wrong_revision_fail() {
    let dir = fixture();
    let policy = load_policy(dir.path()).unwrap();
    let mut audit = complete(dir.path());
    audit.recorded_at = "2027-01-01T00:00:00Z".into();
    assert!(validate_audit(dir.path(), &policy, &audit, now()).is_err());
    let mut audit = complete(dir.path());
    audit.repository_revision = "a".repeat(40);
    assert!(validate_audit(dir.path(), &policy, &audit, now()).is_err());
}
#[test]
fn invalid_duplicate_overdue_and_future_review_metadata_fail() {
    let dir = fixture();
    let baseline = load_policy(dir.path()).unwrap();
    let mut duplicate = baseline.clone();
    duplicate.rules.push(duplicate.rules[0].clone());
    assert!(validate_policy(&duplicate, now().date()).is_err());
    let mut invalid = baseline.clone();
    invalid.rules[0].scope = "anything".into();
    assert!(validate_policy(&invalid, now().date()).is_err());
    let mut overdue = baseline.clone();
    overdue.rules[0].review_due = "2026-10-14".into();
    overdue.rules[0].reviewed_on = "2026-10-01".into();
    assert!(
        validate_policy(&overdue, now().date())
            .unwrap_err()
            .contains("Overdue")
    );
    let mut future = baseline;
    future.rules[0].reviewed_on = "2027-01-01".into();
    assert!(validate_policy(&future, now().date()).is_err());
}

#[test]
fn policy_date_uses_new_york_calendar_across_summer_and_winter_midnight() {
    let rfc3339 = time::format_description::well_known::Rfc3339;
    let cases = [
        ("2026-07-15T03:59:59Z", "2026-07-14"),
        ("2026-07-15T04:00:00Z", "2026-07-15"),
        ("2026-01-01T04:59:59Z", "2025-12-31"),
        ("2026-01-01T05:00:00Z", "2026-01-01"),
    ];
    for (instant, expected_date) in cases {
        let instant = OffsetDateTime::parse(instant, &rfc3339).unwrap();
        assert_eq!(policy_date_at(instant).unwrap().to_string(), expected_date);
    }
}

#[test]
fn primary_validator_rejects_missing_stable_verification_rule() {
    let dir = fixture();
    let mut policy = load_policy(dir.path()).unwrap();
    policy.rules.retain(|rule| rule.id != "AUTH-01");
    assert!(
        validate_policy(&policy, policy_date_at(now()).unwrap())
            .unwrap_err()
            .contains("AUTH-01")
    );
}

#[test]
fn new_audit_binds_the_checkout_head_not_ambient_git_repository_overrides() {
    let checkout = fixture();
    let linked_sandbox = tempfile::tempdir().unwrap();
    let selected = linked_sandbox.path().join("selected-worktree");
    add_linked_worktree(checkout.path(), &selected);
    let alternate = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(
        alternate.path().join("README.md"),
        "Fictional alternate repository\n",
    )
    .unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "README.md"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "Fictional alternate HEAD",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(alternate.path())
                .status()
                .unwrap()
                .success()
        );
    }
    let head = |root: &Path| {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let checkout_head = head(&selected);
    let alternate_head = head(alternate.path());
    assert_ne!(checkout_head, alternate_head);

    let output_path = outside.path().join("audit.json");
    let mut command = Command::new(env!("CARGO_BIN_EXE_toe-tickling"));
    command
        .env("GIT_DIR", alternate.path().join(".git"))
        .env("GIT_WORK_TREE", &selected)
        .env("GIT_COMMON_DIR", alternate.path().join(".git"))
        .env("GIT_INDEX_FILE", alternate.path().join(".git/index"))
        .arg("--root")
        .arg(&selected)
        .args([
            "new-audit",
            "--task",
            "Fictional isolated Git environment regression",
            "--output",
        ])
        .arg(&output_path);
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let audit: Audit = read_json(&output_path).unwrap();
    assert_eq!(audit.repository_revision, checkout_head);

    let mut spoofed = complete(&selected);
    spoofed.repository_revision = alternate_head;
    let spoofed_path = outside.path().join("spoofed.json");
    write_json(&spoofed_path, &spoofed).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_toe-tickling"))
        .arg("--root")
        .arg(&selected)
        .arg("check")
        .arg("--audit")
        .arg(spoofed_path)
        .env("GIT_DIR", alternate.path().join(".git"))
        .env("GIT_WORK_TREE", alternate.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Audit revision does not match checkout HEAD")
    );
}

#[test]
fn every_existing_verification_id_must_survive_policy_updates() {
    let dir = fixture();
    let baseline = load_policy(dir.path()).unwrap();
    for removed in &baseline.rules {
        let mut policy = baseline.clone();
        policy.rules.retain(|rule| rule.id != removed.id);
        assert!(
            validate_policy(&policy, now().date()).is_err(),
            "silently deleted {}",
            removed.id
        );
    }
}

#[test]
fn verification_rules_can_evolve_without_deleting_history() {
    let dir = fixture();
    let mut policy = load_policy(dir.path()).unwrap();
    let mut successor = policy.rules[0].clone();
    successor.id = "AUTH-07".into();
    successor.supersedes = vec![policy.rules[0].id.clone()];
    policy.rules[0].status = "superseded".into();
    policy.rules.push(successor);
    validate_policy(&policy, now().date()).unwrap();
}

#[test]
fn supersession_missing_replacement_active_target_and_cycles_fail() {
    let dir = fixture();
    let baseline = load_policy(dir.path()).unwrap();
    let mut policy = baseline.clone();
    policy.rules[0].status = "superseded".into();
    assert!(validate_policy(&policy, now().date()).is_err());
    let mut policy = baseline.clone();
    policy.rules[1].supersedes = vec![policy.rules[0].id.clone()];
    assert!(validate_policy(&policy, now().date()).is_err());
    let mut policy = baseline;
    policy.rules[0].status = "superseded".into();
    policy.rules[1].status = "superseded".into();
    policy.rules[0].supersedes = vec![policy.rules[1].id.clone()];
    policy.rules[1].supersedes = vec![policy.rules[0].id.clone()];
    assert!(
        validate_policy(&policy, now().date())
            .unwrap_err()
            .contains("cycle")
    );
}
#[test]
fn stale_manifest_cannot_remove_required_files() {
    let dir = fixture();
    let mut manifest = current_manifest(dir.path()).unwrap();
    manifest.documents.remove("AGENTS.md");
    write_json(&dir.path().join("policy/reading-manifest.json"), &manifest).unwrap();
    assert!(check_repo(dir.path(), now().date()).is_err());
}

#[test]
fn guide_build_failure_does_not_persist_an_earlier_generated_file() {
    let dir = fixture();
    let rule_path = dir.path().join("policy/rules.json");
    let mut register: serde_json::Value = read_json(&rule_path).unwrap();
    register["rules"][0]["title"] = serde_json::Value::String("Fictional changed title".into());
    write_json(&rule_path, &register).unwrap();
    fs::write(dir.path().join("policy.json"), "not valid JSON").unwrap();

    let rulebook_before = fs::read(dir.path().join("RULEBOOK.md")).unwrap();
    let guide_before = fs::read(dir.path().join("GUIDEBOOK.md")).unwrap();
    assert!(guide::build(dir.path()).is_err());
    let rulebook_after = fs::read(dir.path().join("RULEBOOK.md")).unwrap();
    let guide_after = fs::read(dir.path().join("GUIDEBOOK.md")).unwrap();
    assert!(
        rulebook_after == rulebook_before,
        "failed generation changed RULEBOOK.md"
    );
    assert!(
        guide_after == guide_before,
        "failed generation changed GUIDEBOOK.md"
    );
}

#[test]
fn guide_build_rejects_output_type_collisions_before_persisting_any_file() {
    let dir = fixture();
    let rule_path = dir.path().join("policy/rules.json");
    let mut register: serde_json::Value = read_json(&rule_path).unwrap();
    register["rules"][0]["title"] = serde_json::Value::String("Fictional changed title".into());
    write_json(&rule_path, &register).unwrap();
    let colliding_output = dir.path().join(".cursor/rules/justin-guidebook.mdc");
    fs::remove_file(&colliding_output).unwrap();
    fs::create_dir(&colliding_output).unwrap();

    let rulebook_before = fs::read(dir.path().join("RULEBOOK.md")).unwrap();
    assert!(guide::build(dir.path()).is_err());
    let rulebook_after = fs::read(dir.path().join("RULEBOOK.md")).unwrap();
    assert!(
        rulebook_after == rulebook_before,
        "output collision partially changed RULEBOOK.md"
    );
}

#[test]
fn broken_link_and_non_rust_source_are_rejected() {
    let dir = fixture();
    fs::write(
        dir.path().join("extra.md"),
        "# Extra\n[missing](absent.md)\n",
    )
    .unwrap();
    assert!(validate_links(dir.path()).is_err());
    fs::remove_file(dir.path().join("extra.md")).unwrap();
    for extension in ["py", "js", "sh", "c", "pl", "go"] {
        let path = dir.path().join(format!("implementation.{extension}"));
        fs::write(&path, "not permitted").unwrap();
        assert!(
            validate_links(dir.path()).is_err(),
            "Failed to reject {extension}"
        );
        fs::remove_file(path).unwrap();
    }
}
#[test]
fn malformed_or_unknown_schema_fields_fail_closed() {
    let dir = fixture();
    let audit = complete(dir.path());
    let path = dir.path().join("bad.json");
    fs::write(&path, "{truncated").unwrap();
    assert!(read_json::<Audit>(&path).is_err());
    let mut value = serde_json::to_value(audit).unwrap();
    value["waive_failures"] = true.into();
    write_json(&path, &value).unwrap();
    assert!(read_json::<Audit>(&path).is_err());
}
#[test]
fn cli_does_not_overwrite_receipts_or_write_them_in_checkout() {
    let dir = fixture();
    let private = tempfile::tempdir().unwrap();
    let output = private.path().join("receipt.json");
    let run = |output: &Path| {
        Command::new(env!("CARGO_BIN_EXE_toe-tickling"))
            .current_dir(dir.path())
            .args([
                "new-audit",
                "--task",
                "Fictional CLI validation task",
                "--output",
            ])
            .arg(output)
            .output()
            .unwrap()
    };
    assert!(run(&output).status.success());
    let original = fs::read(&output).unwrap();
    assert!(!run(&output).status.success());
    assert_eq!(original, fs::read(&output).unwrap());
    assert!(!run(&dir.path().join("receipt.json")).status.success());
    let result = Command::new(env!("CARGO_BIN_EXE_toe-tickling"))
        .current_dir(dir.path())
        .args(["check", "--audit"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
}

#[test]
fn full_guide_and_adapters_cannot_drift() {
    let dir = fixture();
    fs::write(dir.path().join("CLAUDE.md"), "# Stale adapter\n").unwrap();
    assert!(guide::check(dir.path()).is_err());
    guide::build(dir.path()).unwrap();
    guide::check(dir.path()).unwrap();
    fs::write(dir.path().join("GUIDEBOOK.md"), "# Incomplete policy\n").unwrap();
    assert!(guide::check(dir.path()).is_err());
}

#[test]
fn removing_existing_rules_or_restoring_language_exception_fails() {
    let dir = fixture();
    let path = dir.path().join("rules/10-communication.md");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("**COM-001**", "**COM-999**");
    fs::write(path, text).unwrap();
    assert!(guide::build(dir.path()).is_err());
    let dir = fixture();
    let path = dir.path().join("policy.json");
    let mut p: serde_json::Value = read_json(&path).unwrap();
    p["implementation_language"] = "Python".into();
    write_json(&path, &p).unwrap();
    assert!(guide::build(dir.path()).is_err());
}

#[test]
fn build_cannot_silently_refresh_reading_receipts() {
    let dir = fixture();
    let mut policy = load_policy(dir.path()).unwrap();
    policy.rules[0]
        .requirement
        .push_str(" Added reviewed condition.");
    write_json(&dir.path().join("policy/rules.json"), &policy).unwrap();
    guide::build(dir.path()).unwrap();
    assert!(check_repo(dir.path(), now().date()).is_err());
}

#[test]
fn a_bold_prose_reference_cannot_replace_a_rule_declaration() {
    let dir = fixture();
    let path = dir.path().join("rules/60-instruction-audits.md");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("- **AUD-001**", "Example **AUD-001**");
    fs::write(path, text).unwrap();
    assert!(guide::build(dir.path()).is_err());
}

#[test]
fn adoption_cli_runs_from_an_unrelated_directory_and_path_with_spaces() {
    let source = fixture();
    let unrelated = tempfile::tempdir().unwrap();
    let target = tempfile::Builder::new()
        .prefix("project with spaces ")
        .tempdir()
        .unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(target.path())
            .status()
            .unwrap()
            .success()
    );
    let run = |apply: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_toe-tickling"));
        command
            .current_dir(unrelated.path())
            .arg("--root")
            .arg(source.path())
            .arg("adopt")
            .arg(target.path());
        if apply {
            command.arg("--apply");
        }
        command.output().unwrap()
    };
    assert!(run(false).status.success());
    assert!(!target.path().join("AGENTS.md").exists());
    let applied = run(true);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_eq!(
        fs::read(target.path().join(".ai-guidebook/GUIDEBOOK.md")).unwrap(),
        fs::read(source.path().join("GUIDEBOOK.md")).unwrap()
    );
    let repeated = run(true);
    assert!(repeated.status.success());
    let report: serde_json::Value = serde_json::from_slice(&repeated.stdout).unwrap();
    assert_eq!(report["changed"], serde_json::json!([]));
    let head = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(target.path())
        .output()
        .unwrap();
    assert!(!head.status.success(), "Adoption must not create commits");
}

#[test]
fn audit_validation_uses_the_selected_checkout_under_git_redirects() {
    let source = fixture();
    let linked_sandbox = tempfile::tempdir().unwrap();
    let selected = linked_sandbox.path().join("selected-worktree");
    add_linked_worktree(source.path(), &selected);
    let audit = complete(&selected);
    let foreign = tempfile::tempdir().unwrap();
    commit_fixture(foreign.path(), "Fictional external repository");
    let output_dir = tempfile::tempdir().unwrap();
    let receipt_path = output_dir.path().join("receipt.json");
    write_json(&receipt_path, &audit).unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_toe-tickling"))
        .arg("--root")
        .arg(&selected)
        .arg("check")
        .arg("--audit")
        .arg(&receipt_path)
        .env("GIT_DIR", foreign.path().join(".git"))
        .env("GIT_WORK_TREE", &selected)
        .env("GIT_COMMON_DIR", foreign.path().join(".git"))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let alternate_index = Command::new(env!("CARGO_BIN_EXE_toe-tickling"))
        .arg("--root")
        .arg(&selected)
        .arg("check")
        .arg("--audit")
        .arg(&receipt_path)
        .env(
            "GIT_INDEX_FILE",
            foreign.path().join(".git/alternate-index"),
        )
        .output()
        .unwrap();
    let alternate_stderr = String::from_utf8_lossy(&alternate_index.stderr);
    assert!(
        !alternate_index.status.success(),
        "audit validation must fail closed for an alternate Git index"
    );
    assert!(
        alternate_stderr.contains("GIT_INDEX_FILE override"),
        "unexpected alternate-index result: {alternate_stderr}"
    );
}
