// SPDX-License-Identifier: AGPL-3.0-only
use std::{env, fs, path::Path, process::Command};
use toe_tickling::adopt;

#[test]
fn public_apply_pauses_before_any_target_write() {
    let scratch = tempfile::tempdir().unwrap();
    let target = scratch.path().join("target project");
    let outside = scratch.path().join("outside sentinel.txt");
    fs::create_dir(&target).unwrap();
    fs::write(&outside, b"outside data must remain unchanged").unwrap();

    let mut git = Command::new("git");
    git.args(["init", "-q"]).current_dir(&target);
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

    let plan = adopt::plan(Path::new(env!("CARGO_MANIFEST_DIR")), &target).unwrap();
    let error = adopt::apply(&plan).unwrap_err();

    assert!(error.contains("paused"), "unexpected error: {error}");
    assert!(!target.join(".ai-guidebook").exists());
    assert!(!target.join("AGENTS.md").exists());
    assert!(!target.join(".gitignore").exists());
    assert!(!target.join("CLAUDE.md").exists());
    assert_eq!(
        fs::read(&outside).unwrap(),
        b"outside data must remain unchanged"
    );
    assert!(!target.join(".git/.toe-tickling-adoption-v1.json").exists());
}

#[test]
fn public_recover_pauses_before_reading_or_removing_journal() {
    let scratch = tempfile::tempdir().unwrap();
    let target = scratch.path().join("target project");
    let outside = scratch.path().join("outside sentinel.txt");
    fs::create_dir(&target).unwrap();
    fs::write(&outside, b"outside data must remain unchanged").unwrap();

    let mut git = Command::new("git");
    git.args(["init", "-q"]).current_dir(&target);
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

    let journal = target.join(".git/.toe-tickling-adoption-v1.json");
    fs::write(&journal, b"malformed journal must not be read or removed").unwrap();
    let journal_before = fs::read(&journal).unwrap();
    let error = adopt::recover(Path::new(env!("CARGO_MANIFEST_DIR")), &target).unwrap_err();

    assert!(error.contains("paused"), "unexpected error: {error}");
    assert_eq!(fs::read(&journal).unwrap(), journal_before);
    assert_eq!(
        fs::read(&outside).unwrap(),
        b"outside data must remain unchanged"
    );
    assert!(!target.join(".ai-guidebook").exists());
    assert!(!target.join("AGENTS.md").exists());
}
