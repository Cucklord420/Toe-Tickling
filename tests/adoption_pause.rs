// SPDX-License-Identifier: AGPL-3.0-only

use std::{env, fs, path::Path, process::Command};
use toe_tickling::adopt;

fn init_git(target: &Path) {
    let mut git = Command::new("git");
    git.args(["init", "-q"]).current_dir(target);
    for (key, _) in env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            git.env_remove(key);
        }
    }
    assert!(git.output().unwrap().status.success());
}

#[test]
fn public_apply_pauses_before_inspecting_or_creating_target() {
    let scratch = tempfile::tempdir().unwrap();
    let scratch_path = scratch.path().canonicalize().unwrap();
    let target = scratch_path.join("target project");
    let outside = scratch_path.join("outside sentinel.txt");
    fs::write(&outside, b"outside data must remain unchanged").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_toe-tickling"))
        .args(["adopt", target.to_str().unwrap(), "--apply"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "unexpected apply success JSON");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("paused"), "unexpected error: {error}");
    assert!(!target.exists());
    assert_eq!(
        fs::read(&outside).unwrap(),
        b"outside data must remain unchanged"
    );
}

#[test]
fn public_recover_pauses_before_reading_or_removing_journal() {
    let scratch = tempfile::tempdir().unwrap();
    let scratch_path = scratch.path().canonicalize().unwrap();
    let target = scratch_path.join("target project");
    let outside = scratch_path.join("outside sentinel.txt");
    fs::create_dir(&target).unwrap();
    fs::write(&outside, b"outside data must remain unchanged").unwrap();
    init_git(&target);

    let journal = target.join(".git/.toe-tickling-adoption-v1.json");
    fs::write(&journal, b"malformed journal must not be read or removed").unwrap();
    let journal_before = fs::read(&journal).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_toe-tickling"))
        .args(["adopt", target.to_str().unwrap(), "--recover"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "unexpected recovery success JSON");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("paused"), "unexpected error: {error}");
    let api_error = adopt::recover(Path::new(env!("CARGO_MANIFEST_DIR")), &target).unwrap_err();
    assert!(
        api_error.contains("paused"),
        "unexpected API error: {api_error}"
    );
    assert_eq!(fs::read(&journal).unwrap(), journal_before);
    assert_eq!(
        fs::read(&outside).unwrap(),
        b"outside data must remain unchanged"
    );
    assert!(!target.join(".ai-guidebook").exists());
    assert!(!target.join("AGENTS.md").exists());
}
