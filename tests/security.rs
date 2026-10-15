use std::{fs, path::Path, process::Command};
use tempfile::TempDir;
use toe_tickling::security::check;

const WORKFLOW: &str = "name: audit\npermissions:\n  contents: read\njobs:\n  audit:\n    runs-on: ubuntu-latest\n    timeout-minutes: 10\n    steps:\n      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683\n        with:\n          persist-credentials: false\n      - run: cargo test --locked\n";

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn fixture() -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    write(directory.path(), ".github/workflows/validate.yml", WORKFLOW);
    write(
        directory.path(),
        "README.md",
        "# Fictional public fixture\n",
    );
    directory
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "Git fixture operation failed");
}

#[test]
fn public_rust_and_declarative_formats_are_accepted() {
    let directory = fixture();
    for path in [
        "src/main.rs",
        "Cargo.toml",
        "Cargo.lock",
        "data.json",
        "LICENSE",
        ".gitignore",
        ".gitattributes",
        ".cursor/rules/book.mdc",
        ".env.example",
    ] {
        write(directory.path(), path, "Fictional public text\n");
    }
    check(directory.path()).unwrap();
}

#[test]
fn alternative_executable_formats_and_unknown_files_are_rejected() {
    for path in [
        "tool.py",
        "tool.sh",
        "tool.js",
        "tool.ts",
        "tool",
        "archive.zip",
    ] {
        let directory = fixture();
        write(directory.path(), path, "fictional content");
        assert!(
            check(directory.path())
                .unwrap_err()
                .contains("Executable source must be Rust")
        );
    }
}

#[test]
fn git_ignored_local_context_is_never_scanned() {
    let directory = fixture();
    git(directory.path(), &["init", "-q"]);
    write(
        directory.path(),
        ".gitignore",
        "/private/\n/.private/\n/.local/\n/.env\n/CLAUDE.local.md\n",
    );
    git(directory.path(), &["add", "."]);
    for path in [
        "private/context.py",
        ".private/context.md",
        ".local/context.md",
        ".env",
        "CLAUDE.local.md",
    ] {
        // Invalid UTF-8 verifies that ignored context was not even opened as text.
        let path = directory.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, [0xff, 0xfe]).unwrap();
    }
    check(directory.path()).unwrap();
}

#[test]
fn tracked_private_paths_remain_rejected_even_when_ignored() {
    for path in [
        "private/context.md",
        ".private/context.md",
        ".local/context.md",
        "PRIVATE.local.md",
        "CLAUDE.local.md",
        ".env",
        ".env.production",
    ] {
        let directory = fixture();
        git(directory.path(), &["init", "-q"]);
        write(
            directory.path(),
            ".gitignore",
            "/private/\n/.private/\n/.local/\n/.env*\n*.local.md\n",
        );
        write(directory.path(), path, "Fictional private placeholder");
        git(directory.path(), &["add", "--force", path]);
        assert!(
            check(directory.path())
                .unwrap_err()
                .contains("Private path"),
            "Accepted {path}"
        );
    }
}

#[test]
fn unignored_untracked_private_context_is_rejected() {
    let directory = fixture();
    git(directory.path(), &["init", "-q"]);
    write(
        directory.path(),
        "private/context.md",
        "Fictional placeholder",
    );
    assert!(
        check(directory.path())
            .unwrap_err()
            .contains("Private path")
    );
}

#[test]
fn git_enumerates_new_public_files_and_skips_tracked_deletions() {
    let directory = fixture();
    git(directory.path(), &["init", "-q"]);
    write(directory.path(), "removed.md", "Fictional removed source");
    git(directory.path(), &["add", "."]);
    fs::remove_file(directory.path().join("removed.md")).unwrap();
    check(directory.path()).unwrap();
    write(
        directory.path(),
        "untracked.py",
        "Fictional new implementation",
    );
    assert!(
        check(directory.path())
            .unwrap_err()
            .contains("Executable source must be Rust")
    );
}

#[test]
fn build_and_git_internal_artifacts_are_excluded() {
    let directory = fixture();
    write(
        directory.path(),
        "target/dependency/build.py",
        "Fictional inherited dependency",
    );
    write(
        directory.path(),
        "nested/.git/internal.bin",
        "Fictional internal artifact",
    );
    check(directory.path()).unwrap();
}

#[test]
fn ignored_untracked_rust_output_and_backups_stay_out_of_the_scan() {
    let directory = fixture();
    git(directory.path(), &["init", "-q"]);
    write(directory.path(), ".gitignore", "target/\n*.rs.bk\n");
    for path in [
        "target/debug/toe-tickling",
        "rust/audit/target/debug/guidebook-audit",
        "src/main.rs.bk",
    ] {
        let path = directory.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, [0x7f, b'E', b'L', b'F', 0xff, 0xfe]).unwrap();
    }
    check(directory.path()).unwrap();
}

#[test]
fn forced_tracked_rust_build_output_is_rejected_even_when_ignored() {
    for path in [
        "target/debug/toe-tickling",
        "rust/audit/target/debug/guidebook-audit",
        "nested/target/output.json",
    ] {
        let directory = fixture();
        git(directory.path(), &["init", "-q"]);
        write(directory.path(), ".gitignore", "target/\n");
        write(directory.path(), path, "Fictional build output");
        git(directory.path(), &["add", "--force", path]);
        assert!(
            check(directory.path())
                .unwrap_err()
                .contains("Tracked or non-ignored Rust build output"),
            "Accepted tracked output {path}"
        );
    }
}

#[test]
fn nonignored_untracked_rust_build_output_is_rejected_before_reading() {
    let directory = fixture();
    git(directory.path(), &["init", "-q"]);
    let output = directory
        .path()
        .join("rust/audit/target/debug/guidebook-audit");
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::write(output, [0xff, 0xfe]).unwrap();
    assert!(
        check(directory.path())
            .unwrap_err()
            .contains("Tracked or non-ignored Rust build output")
    );
}

#[test]
fn tracked_or_unignored_rust_backups_are_rejected() {
    for tracked in [false, true] {
        let directory = fixture();
        git(directory.path(), &["init", "-q"]);
        write(
            directory.path(),
            "src/main.rs.bk",
            "Fictional stale source backup",
        );
        if tracked {
            write(directory.path(), ".gitignore", "*.rs.bk\n");
            git(directory.path(), &["add", "--force", "src/main.rs.bk"]);
        }
        assert!(
            check(directory.path())
                .unwrap_err()
                .contains("Executable source must be Rust")
        );
    }
}

#[test]
fn selected_credentials_and_contacts_fail_without_leaking_matches() {
    let samples = [
        format!("ghp_{}", "a".repeat(24)),
        format!("github_pat_{}", "b".repeat(24)),
        format!("sk-{}", "c".repeat(24)),
        format!("gho_{}", "d".repeat(24)),
        format!("ghu_{}", "e".repeat(24)),
        format!("ghs_{}", "f".repeat(24)),
        format!("ghr_{}", "g".repeat(24)),
        format!("sk-proj-{}", "h".repeat(24)),
        format!("sk-svcacct-{}", "i".repeat(24)),
        format!("glpat-{}", "j".repeat(24)),
        format!("xoxb-{}", "k".repeat(24)),
        format!("AKIA{}", "A".repeat(16)),
        format!("ASIA{}", "B".repeat(16)),
        ["-----BEGIN ", "OPENSSH ", "PRIVATE KEY-----"].concat(),
        format!("person@{}.com", "fictional-company"),
    ];
    for sample in samples {
        let directory = fixture();
        write(directory.path(), "public.md", &sample);
        let error = check(directory.path()).unwrap_err();
        assert!(error.contains("Potential private credential/contact"));
        assert!(!error.contains(&sample));
    }
}

#[test]
fn reserved_fictional_contacts_are_allowed() {
    let directory = fixture();
    write(
        directory.path(),
        "public.md",
        "fixture@example.invalid and fixture@sub.example.invalid\n",
    );
    check(directory.path()).unwrap();
}

#[test]
fn parent_links_and_percent_encoded_utf8_resolve_within_the_root() {
    let directory = fixture();
    write(
        directory.path(),
        "docs/café note.md",
        "# Fictional target\n",
    );
    write(
        directory.path(),
        "docs/links.md",
        "[root](../README.md#part)\n[UTF-8](caf%C3%A9%20note.md)\n[space](<café note.md>)\n",
    );
    check(directory.path()).unwrap();
}

#[test]
fn external_links_do_not_require_live_network_access() {
    let directory = fixture();
    write(
        directory.path(),
        "links.md",
        "[https](https://unreachable.invalid/path)\n[scheme](custom:somewhere)\n[remote](//unreachable.invalid/path)\n[fragment](#part)\n",
    );
    check(directory.path()).unwrap();
}

#[test]
fn broken_escaping_and_malformed_local_links_are_rejected() {
    for target in [
        "missing.md",
        "../outside.md",
        "%2e%2e/outside.md",
        "%ff.md",
        "%zz.md",
        "/README.md",
    ] {
        let directory = fixture();
        write(
            directory.path(),
            "links.md",
            &format!("[target]({target})\n"),
        );
        assert!(check(directory.path()).is_err(), "Accepted {target}");
    }
}

#[cfg(unix)]
#[test]
fn public_symlink_files_directories_and_root_ancestors_are_rejected() {
    use std::os::unix::fs::symlink;
    for tracked in [false, true] {
        let directory = fixture();
        git(directory.path(), &["init", "-q"]);
        symlink(
            directory.path().join("README.md"),
            directory.path().join("alias.md"),
        )
        .unwrap();
        if tracked {
            git(directory.path(), &["add", "."]);
        }
        assert!(check(directory.path()).unwrap_err().contains("symlink"));
    }
    let directory = fixture();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), directory.path().join("docs")).unwrap();
    assert!(check(directory.path()).unwrap_err().contains("symlink"));
    let parent = tempfile::tempdir().unwrap();
    symlink(directory.path(), parent.path().join("alias")).unwrap();
    assert!(
        check(&parent.path().join("alias"))
            .unwrap_err()
            .contains("symlink")
    );
}

#[cfg(unix)]
#[test]
fn ignored_symlinks_stay_private_but_public_links_cannot_follow_them() {
    use std::os::unix::fs::symlink;
    let directory = fixture();
    git(directory.path(), &["init", "-q"]);
    write(directory.path(), ".gitignore", "/local\n");
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "note.md", "Fictional local context");
    symlink(outside.path(), directory.path().join("local")).unwrap();
    check(directory.path()).unwrap();
    write(directory.path(), "links.md", "[local](local/note.md)\n");
    assert!(check(directory.path()).unwrap_err().contains("symlink"));
}

#[test]
fn ci_controls_cannot_be_satisfied_by_comments_or_another_checkout() {
    let mutations = [
        WORKFLOW.replace(
            "permissions:\n  contents: read",
            "# contents: read\npermissions: write-all",
        ),
        WORKFLOW.replace(
            "persist-credentials: false",
            "# persist-credentials: false\n          persist-credentials: true",
        ),
        WORKFLOW.replace("11bd71901bbe5b1630ceea73d27597364c9af683", "v4"),
        WORKFLOW.replace("contents: read", "contents: write"),
        WORKFLOW.replace(
            "    runs-on:",
            "    permissions:\n      contents: write\n    runs-on:",
        ),
        format!(
            "{WORKFLOW}      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683\n"
        ),
        format!("{WORKFLOW}      - uses: vendor/action@main\n"),
    ];
    for workflow in mutations {
        let directory = fixture();
        write(
            directory.path(),
            ".github/workflows/validate.yml",
            &workflow,
        );
        assert!(check(directory.path()).is_err());
    }
}

#[test]
fn every_job_and_reusable_workflow_keeps_permission_and_pin_controls() {
    let directory = fixture();
    let workflow = format!(
        "{WORKFLOW}  other:\n    permissions:\n      contents: read\n    uses: vendor/repository/.github/workflows/test.yml@{}\n",
        "a".repeat(40)
    );
    write(
        directory.path(),
        ".github/workflows/validate.yml",
        &workflow,
    );
    check(directory.path()).unwrap();
    write(
        directory.path(),
        ".github/workflows/validate.yml",
        &workflow.replace(&"a".repeat(40), "main"),
    );
    assert!(
        check(directory.path())
            .unwrap_err()
            .contains("full commit pins")
    );
}

#[test]
fn missing_or_invalid_workflow_fails_closed() {
    for workflow in [
        "",
        "not: [valid YAML",
        "permissions:\n  contents: read\njobs: {}\n",
    ] {
        let directory = fixture();
        write(directory.path(), ".github/workflows/validate.yml", workflow);
        assert!(check(directory.path()).is_err());
    }
    let directory = fixture();
    fs::remove_file(directory.path().join(".github/workflows/validate.yml")).unwrap();
    assert!(check(directory.path()).is_err());
}

#[test]
fn ci_run_steps_require_bounded_rust_tooling_without_embedded_scripts() {
    for command in [
        "python3 scripts/tool.py",
        "bash scripts/tool.sh",
        "cargo test; echo bypass",
        "cargo test && cargo check",
        "cargo test $(command)",
        "|\n          cargo test\n          cargo check",
    ] {
        let directory = fixture();
        write(
            directory.path(),
            ".github/workflows/validate.yml",
            &WORKFLOW.replace("cargo test --locked", command),
        );
        assert!(
            check(directory.path())
                .unwrap_err()
                .contains("single Rust tooling")
        );
    }
    for workflow in [
        WORKFLOW.replace("    timeout-minutes: 10\n", ""),
        WORKFLOW.replace("timeout-minutes: 10", "timeout-minutes: 11"),
        WORKFLOW.replace("      - run:", "      - shell: python\n        run:"),
        WORKFLOW.replace("jobs:", "defaults:\n  run:\n    shell: python {0}\njobs:"),
    ] {
        let directory = fixture();
        write(
            directory.path(),
            ".github/workflows/validate.yml",
            &workflow,
        );
        assert!(check(directory.path()).is_err());
    }
}

#[test]
fn every_workflow_is_checked_including_second_yaml_files() {
    for extension in ["yml", "yaml"] {
        for workflow in [
            WORKFLOW.replace("contents: read", "contents: write"),
            WORKFLOW.replace("11bd71901bbe5b1630ceea73d27597364c9af683", "v4"),
            WORKFLOW.replace("persist-credentials: false", "persist-credentials: true"),
            WORKFLOW.replace("cargo test --locked", "python3 scripts/tool.py"),
            format!("on: pull_request_target\n{WORKFLOW}"),
        ] {
            let directory = fixture();
            write(
                directory.path(),
                &format!(".github/workflows/second.{extension}"),
                &workflow,
            );
            assert!(check(directory.path()).is_err());
        }
    }
}

#[test]
fn privileged_ci_triggers_and_secret_access_require_review() {
    for workflow in [
        format!("on: pull_request_target\n{WORKFLOW}"),
        format!("on: [push, workflow_run]\n{WORKFLOW}"),
        format!("on:\n  pull_request_target:\n    types: [opened]\n{WORKFLOW}"),
        WORKFLOW.replace("    runs-on:", "    secrets: inherit\n    runs-on:"),
        WORKFLOW.replace(
            "    runs-on:",
            "    env:\n      TOKEN: '${{ secrets.TOKEN }}'\n    runs-on:",
        ),
        WORKFLOW.replace(
            "    runs-on:",
            "    env:\n      TOKEN: '${{ toJSON(secrets) }}'\n    runs-on:",
        ),
        format!(
            "on:\n  workflow_call:\n    secrets:\n      TOKEN:\n        required: true\n{WORKFLOW}"
        ),
    ] {
        let directory = fixture();
        write(
            directory.path(),
            ".github/workflows/validate.yml",
            &workflow,
        );
        assert!(check(directory.path()).is_err());
    }
}

#[test]
fn safe_ci_comments_names_and_readonly_overrides_are_not_privilege_flags() {
    let directory = fixture();
    let workflow = format!(
        "# pull_request_target workflow_run secrets are prohibited\non: [push, pull_request]\n{}",
        WORKFLOW.replace("    runs-on:", "    name: Review secrets documentation\n    permissions:\n      contents: read\n      issues: none\n    runs-on:")
    );
    write(
        directory.path(),
        ".github/workflows/validate.yml",
        &workflow,
    );
    write(
        directory.path(),
        ".github/workflows/reusable.yaml",
        &format!(
            "on: workflow_dispatch\npermissions:\n  contents: read\njobs:\n  call:\n    uses: owner/repo/.github/workflows/check.yml@{}\n",
            "a".repeat(40)
        ),
    );
    check(directory.path()).unwrap();
}
