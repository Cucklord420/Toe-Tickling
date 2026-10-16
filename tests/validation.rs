use std::{collections::BTreeMap, fs, path::Path, process::Command};
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
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
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
    let audit = complete(dir.path());
    fs::write(dir.path().join("AGENTS.md"), "# Changed instructions\n").unwrap();
    assert!(
        check_repo(dir.path(), now().date())
            .unwrap_err()
            .contains("manifest")
    );
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
    let path = dir.path().join("bad.json");
    fs::write(&path, "{truncated").unwrap();
    assert!(read_json::<Audit>(&path).is_err());
    let mut value = serde_json::to_value(complete(dir.path())).unwrap();
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
