use std::{fs, path::Path, process::Command};
use tempfile::TempDir;
use toe_tickling::security::check;
#[cfg(unix)]
use toe_tickling::security::checked_root;
use toe_tickling::{READING_FILES, current_manifest, guide, load_policy, render, write_json};

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

fn full_fixture() -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in READING_FILES.iter().chain(guide::SOURCES.iter()) {
        let target = directory.path().join(path);
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
        let target = directory.path().join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(file), target).unwrap();
    }
    fs::write(directory.path().join(".gitignore"), "/README.md\n").unwrap();
    let policy = load_policy(directory.path()).unwrap();
    fs::write(directory.path().join("RULEBOOK.md"), render(&policy)).unwrap();
    guide::build(directory.path()).unwrap();
    write_json(
        &directory.path().join("policy/reading-manifest.json"),
        &current_manifest(directory.path()).unwrap(),
    )
    .unwrap();
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

fn git_isolated(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Git fixture operation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
        "PRIVATE.Local.md",
        "CLAUDE.local.md",
        "claude.local.md",
        ".env",
        ".env.production",
        ".ENV.md",
        ".ENV",
        ".ENV.example",
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
fn ambient_git_redirects_cannot_hide_tracked_ignored_private_files() {
    if let Some(root) = std::env::var_os("TOE_TICKLING_SECURITY_CHILD_ROOT") {
        let result = check(Path::new(&root));
        let expected_error = if std::env::var_os("GIT_INDEX_FILE").is_some() {
            "GIT_INDEX_FILE override"
        } else {
            "Private path"
        };
        assert!(
            result
                .as_ref()
                .is_err_and(|error| error.contains(expected_error)),
            "expected {expected_error} rejection, got {result:?}"
        );
        return;
    }

    let directory = fixture();
    git_isolated(directory.path(), &["init", "-q"]);
    write(directory.path(), ".gitignore", "/.env\n");
    write(directory.path(), ".env", "Fictional private fixture\n");
    git_isolated(directory.path(), &["add", "."]);
    git_isolated(directory.path(), &["add", "--force", ".env"]);
    git_isolated(
        directory.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "synthetic private fixture",
        ],
    );

    let linked_sandbox = tempfile::tempdir().unwrap();
    let linked = linked_sandbox.path().join("selected-worktree");
    let linked_path = linked.to_string_lossy().into_owned();
    git_isolated(
        directory.path(),
        &["worktree", "add", "--detach", &linked_path, "HEAD"],
    );
    assert!(
        linked.join(".git").is_file(),
        "fixture is not a linked worktree"
    );

    let foreign = tempfile::tempdir().unwrap();
    git_isolated(foreign.path(), &["init", "-q"]);
    for redirect in ["git-dir", "work-tree", "common-dir", "index-file"] {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "ambient_git_redirects_cannot_hide_tracked_ignored_private_files",
                "--nocapture",
            ])
            .env("TOE_TICKLING_SECURITY_CHILD_ROOT", &linked)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_CONFIG_COUNT")
            .env_remove("GIT_CONFIG_KEY_0")
            .env_remove("GIT_CONFIG_VALUE_0");
        match redirect {
            "git-dir" => {
                child
                    .env("GIT_DIR", foreign.path().join(".git"))
                    .env("GIT_WORK_TREE", &linked);
            }
            "work-tree" => {
                child.env("GIT_WORK_TREE", foreign.path());
            }
            "common-dir" => {
                child.env("GIT_COMMON_DIR", foreign.path().join(".git"));
            }
            "index-file" => {
                child.env(
                    "GIT_INDEX_FILE",
                    foreign.path().join(".git/alternate-index"),
                );
            }
            _ => unreachable!(),
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{redirect} child failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ambient_git_redirects_cannot_hide_tracked_ignored_private_files",
            "--nocapture",
        ])
        .env("TOE_TICKLING_SECURITY_CHILD_ROOT", &linked)
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.worktree")
        .env("GIT_CONFIG_VALUE_0", foreign.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "config redirect child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unignored_untracked_private_context_is_rejected() {
    for path in [
        "private/context.md",
        "PRIVATE.Local.md",
        "claude.local.md",
        ".ENV.md",
        ".ENV",
        ".ENV.example",
    ] {
        let directory = fixture();
        git(directory.path(), &["init", "-q"]);
        write(directory.path(), path, "Fictional placeholder");
        assert!(
            check(directory.path())
                .unwrap_err()
                .contains("Private path"),
            "Accepted untracked private path {path}"
        );
    }
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
fn staged_credentials_are_rejected_after_the_worktree_is_cleaned_or_removed() {
    let fictional_token = ["ghp_", "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345"].concat();
    for remove_worktree_file in [false, true] {
        let directory = fixture();
        git(directory.path(), &["init", "-q"]);
        git(directory.path(), &["add", "."]);
        write(
            directory.path(),
            "README.md",
            &format!("Fictional credential {fictional_token}\n"),
        );
        git(directory.path(), &["add", "README.md"]);
        if remove_worktree_file {
            fs::remove_file(directory.path().join("README.md")).unwrap();
        } else {
            write(
                directory.path(),
                "README.md",
                "Harmless public documentation.\n",
            );
        }
        let error = check(directory.path()).unwrap_err();
        assert!(error.contains("Potential private credential/contact"));
        assert!(!error.contains(&fictional_token));
    }
}

#[test]
fn staged_renamed_file_is_checked_from_its_index_blob() {
    let fictional_token = ["ghp_", "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345"].concat();
    let directory = fixture();
    git_isolated(directory.path(), &["init", "-q"]);
    git_isolated(directory.path(), &["add", "."]);
    git_isolated(
        directory.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "Fictional baseline",
        ],
    );

    write(
        directory.path(),
        "README.md",
        &format!("Fictional public fixture\n{fictional_token}\n"),
    );
    git_isolated(directory.path(), &["add", "README.md"]);
    git_isolated(directory.path(), &["mv", "README.md", "renamed.md"]);
    write(directory.path(), "renamed.md", "Fictional public fixture\n");

    let error = check(directory.path()).unwrap_err();
    assert!(error.contains("Potential private credential/contact"));
    assert!(!error.contains(&fictional_token));
}

#[test]
fn staged_symlink_mode_is_rejected_even_when_worktree_is_regular() {
    let directory = fixture();
    git_isolated(directory.path(), &["init", "-q"]);
    git_isolated(directory.path(), &["add", "."]);
    git_isolated(
        directory.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "Fictional baseline",
        ],
    );

    let blob = Command::new("git")
        .args(["hash-object", "-w", "README.md"])
        .current_dir(directory.path())
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap();
    assert!(blob.status.success());
    let oid = String::from_utf8(blob.stdout).unwrap().trim().to_owned();
    let cache_info = format!("120000,{oid},README.md");
    git_isolated(
        directory.path(),
        &["update-index", "--add", "--cacheinfo", &cache_info],
    );

    let error = check(directory.path()).unwrap_err();
    assert!(error.contains("regular file, not a symlink or special entry"));
}

#[test]
fn scanner_fails_closed_on_ambient_git_index_override() {
    let fictional_token = ["ghp_", "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345"].concat();
    let directory = full_fixture();
    git(directory.path(), &["init", "-q"]);
    git(directory.path(), &["add", "."]);
    write(
        directory.path(),
        "README.md",
        &format!("Fictional credential {fictional_token}\n"),
    );
    write_json(
        &directory.path().join("policy/reading-manifest.json"),
        &current_manifest(directory.path()).unwrap(),
    )
    .unwrap();
    git(directory.path(), &["add", "policy/reading-manifest.json"]);
    git(directory.path(), &["add", "--force", "README.md"]);
    let alternate_index_dir = tempfile::tempdir().unwrap();
    let alternate_index = alternate_index_dir.path().join("index");
    assert!(
        Command::new("git")
            .args(["read-tree", "--empty"])
            .current_dir(directory.path())
            .env("GIT_INDEX_FILE", &alternate_index)
            .status()
            .unwrap()
            .success()
    );

    let output = Command::new(env!("CARGO_BIN_EXE_toe-tickling"))
        .arg("check")
        .current_dir(directory.path())
        .env("GIT_INDEX_FILE", alternate_index)
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !output.status.success(),
        "alternate index hid tracked content"
    );
    assert!(
        stderr.contains("GIT_INDEX_FILE override"),
        "unexpected scanner result: {stderr}"
    );
    assert!(!stderr.contains(&fictional_token));
}

#[test]
fn alternate_index_only_staged_credentials_fail_closed() {
    let fictional_token = ["ghp_", "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345"].concat();
    let directory = full_fixture();
    git_isolated(directory.path(), &["init", "-q"]);
    git_isolated(directory.path(), &["add", "."]);
    git_isolated(directory.path(), &["add", "--force", "README.md"]);

    let alternate_index_dir = tempfile::tempdir().unwrap();
    let alternate_index = alternate_index_dir.path().join("index");
    fs::copy(directory.path().join(".git/index"), &alternate_index).unwrap();

    let readme = directory.path().join("README.md");
    let original = fs::read(&readme).unwrap();
    fs::write(&readme, format!("Fictional credential {fictional_token}\n")).unwrap();
    let mut stage_alternate = Command::new("git");
    stage_alternate
        .args(["add", "--force", "README.md"])
        .current_dir(directory.path());
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_CEILING_DIRECTORIES",
        "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    ] {
        stage_alternate.env_remove(variable);
    }
    assert!(
        stage_alternate
            .env("GIT_INDEX_FILE", &alternate_index)
            .status()
            .unwrap()
            .success()
    );
    fs::write(&readme, original).unwrap();

    let mut scan = Command::new(env!("CARGO_BIN_EXE_toe-tickling"));
    scan.arg("check").current_dir(directory.path());
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_CEILING_DIRECTORIES",
        "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    ] {
        scan.env_remove(variable);
    }
    let output = scan
        .env("GIT_INDEX_FILE", &alternate_index)
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !output.status.success(),
        "alternate-index-only staged content must fail closed"
    );
    assert!(
        stderr.contains("GIT_INDEX_FILE override"),
        "unexpected scanner result: {stderr}"
    );
    assert!(!stderr.contains(&fictional_token));
}

#[test]
fn reference_style_local_markdown_links_are_validated() {
    let directory = fixture();
    write(
        directory.path(),
        "guide.md",
        "[guide]: existing.md\n\n[guide]\n",
    );
    write(directory.path(), "existing.md", "Fictional target\n");
    check(directory.path()).unwrap();

    write(
        directory.path(),
        "guide.md",
        "[guide]: missing.md\n\n[guide]\n",
    );
    assert!(
        check(directory.path())
            .unwrap_err()
            .contains("Broken or escaping local Markdown link")
    );
}

#[test]
fn markdown_link_forms_resolve_local_destinations() {
    for markdown in [
        "[guide](existing.md)",
        "![guide](existing.md)",
        "[text][guide]\n\n[guide]: existing.md",
        "![text][guide]\n\n[guide]: existing.md",
        "[guide][]\n\n[guide]: existing.md",
        "![guide][]\n\n[guide]: existing.md",
        "[guide]\n\n[guide]: existing.md",
        "![guide]\n\n[guide]: existing.md",
        "[Mixed   Label]\n\n[mIxEd label]: existing.md",
        "[STRASSE]\n\n[straße]: existing.md",
        "[unused]: existing.md",
        "> [nested]: existing.md",
    ] {
        let directory = fixture();
        write(directory.path(), "existing.md", "Fictional target\n");
        write(directory.path(), "links.md", markdown);
        check(directory.path()).unwrap_or_else(|error| panic!("{markdown}: {error}"));
    }
}

#[test]
fn markdown_link_forms_reject_missing_and_escaping_destinations() {
    for target in ["missing.md", "../outside.md", "%2e%2e/outside.md", "%zz.md"] {
        for markdown in [
            format!("[guide]({target})"),
            format!("![guide]({target})"),
            format!("[text][guide]\n\n[guide]: {target}"),
            format!("![text][guide]\n\n[guide]: {target}"),
            format!("[guide][]\n\n[guide]: {target}"),
            format!("![guide][]\n\n[guide]: {target}"),
            format!("[guide]\n\n[guide]: {target}"),
            format!("![guide]\n\n[guide]: {target}"),
            format!("[unused]: {target}"),
            format!("> [nested]: {target}"),
        ] {
            let directory = fixture();
            write(directory.path(), "links.md", &markdown);
            assert!(check(directory.path()).is_err(), "Accepted {markdown}");
        }
    }
}

#[test]
fn undefined_markdown_references_are_literal_text_without_destinations() {
    for markdown in [
        "[text][unknown]",
        "![text][unknown]",
        "[unknown][]",
        "![unknown][]",
        "[unknown]",
        "![unknown]",
    ] {
        let directory = fixture();
        write(directory.path(), "links.md", markdown);
        check(directory.path()).unwrap();
    }
}

#[test]
fn markdown_duplicate_labels_use_the_first_definition() {
    let directory = fixture();
    // CommonMark shadows later duplicates, including their destinations.
    write(
        directory.path(),
        "links.md",
        "[guide]\n\n[guide]: README.md\n[GUIDE]: missing.md\n",
    );
    check(directory.path()).unwrap();
    write(
        directory.path(),
        "links.md",
        "[guide]\n\n[guide]: missing.md\n[GUIDE]: README.md\n",
    );
    assert!(
        check(directory.path())
            .unwrap_err()
            .contains("Broken or escaping local Markdown link")
    );
}

#[test]
fn markdown_destinations_decode_escapes_entities_and_uri_delimiters() {
    let directory = fixture();
    for target in ["note (draft).md", "café note.md", "a&b.md", "part#one.md"] {
        write(directory.path(), target, "Fictional target\n");
    }
    write(
        directory.path(),
        "links.md",
        concat!(
            "[inline](note\\(draft\\).md)\n",
            "[angle](<note (draft).md> \"Fictional title\")\n",
            "[encoded][utf8]\n\n[utf8]: caf%C3%A9%20note.md?raw=1#part\n",
            "[entity]: a&amp;b.md\n",
            "[hash]: part%23one.md#section\n",
            "[escaped]: note\\(draft\\).md\n",
            "[angle reference]: <note (draft).md> 'Fictional title'\n",
        ),
    );
    // Escaped parentheses do not add a space to the destination.
    write(directory.path(), "note(draft).md", "Fictional target\n");
    check(directory.path()).unwrap();
}

#[test]
fn markdown_code_examples_and_escaped_brackets_are_literal() {
    let directory = fixture();
    write(
        directory.path(),
        "links.md",
        concat!(
            "`[inline](missing.md)` and `[unknown]`\n\n",
            "``![image][unknown]``\n\n",
            "```markdown\n[guide](missing.md)\n[unknown]\n[unused]: missing.md\n```\n\n",
            "~~~markdown\n![guide](missing.md)\n[unknown]: missing.md\n~~~\n\n",
            "    [indented](missing.md)\n    [unused]: missing.md\n\n",
            "\\[escaped\\] and \\[text\\](missing.md)\n",
        ),
    );
    check(directory.path()).unwrap();
}

#[test]
fn external_markdown_reference_definitions_and_usages_are_skipped() {
    let directory = fixture();
    write(
        directory.path(),
        "links.md",
        concat!(
            "[https]\n![custom][]\n[text][remote]\n[fragment]\n\n",
            "[https]: <https://unreachable.invalid/a%zz>\n",
            "[custom]: custom:somewhere\n",
            "[remote]: //unreachable.invalid/path\n",
            "[fragment]: #part\n",
            "[unused]: https://unreachable.invalid/missing\n",
        ),
    );
    check(directory.path()).unwrap();
}

#[test]
fn markdown_reference_cannot_resolve_an_existing_target_outside_root() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("public");
    write(&root, ".github/workflows/validate.yml", WORKFLOW);
    write(directory.path(), "outside.md", "Fictional outside target\n");
    write(&root, "links.md", "[guide]\n\n[guide]: ../outside.md\n");
    assert!(
        check(&root)
            .unwrap_err()
            .contains("Broken or escaping local Markdown link")
    );
}

#[test]
fn staged_markdown_reference_is_checked_when_worktree_reference_is_valid() {
    let directory = fixture();
    git(directory.path(), &["init", "-q"]);
    write(
        directory.path(),
        "links.md",
        "[guide]\n\n[guide]: missing.md\n",
    );
    git(directory.path(), &["add", "."]);
    write(
        directory.path(),
        "links.md",
        "[guide]\n\n[guide]: README.md\n",
    );
    assert!(
        check(directory.path())
            .unwrap_err()
            .contains("Broken or escaping local Markdown link")
    );
}

#[test]
fn staged_workflow_is_checked_even_when_worktree_workflow_is_safe() {
    let directory = fixture();
    git(directory.path(), &["init", "-q"]);
    git(directory.path(), &["add", "."]);
    let staged_unsafe = WORKFLOW.replace("contents: read", "contents: write");
    write(
        directory.path(),
        ".github/workflows/validate.yml",
        &staged_unsafe,
    );
    git(directory.path(), &["add", ".github/workflows/validate.yml"]);
    write(directory.path(), ".github/workflows/validate.yml", WORKFLOW);
    assert!(
        check(directory.path())
            .unwrap_err()
            .contains("explicit read-only contents permissions")
    );
}

#[test]
fn staged_workflow_deletion_is_not_hidden_by_an_unstaged_copy() {
    let directory = fixture();
    git(directory.path(), &["init", "-q"]);
    git(directory.path(), &["add", "."]);
    git(
        directory.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "Fictional baseline",
        ],
    );
    git(
        directory.path(),
        &["rm", "--cached", "--", ".github/workflows/validate.yml"],
    );

    assert!(
        check(directory.path())
            .unwrap_err()
            .contains("staged deletion of required CI workflow")
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
fn dot_and_parent_segments_cannot_hide_a_root_symlink_prefix() {
    use std::os::unix::fs::symlink;
    let directory = fixture();
    check(directory.path()).unwrap();
    assert_eq!(checked_root(directory.path()).unwrap(), directory.path());
    let parent = tempfile::tempdir().unwrap();
    let alias = parent.path().join("alias");
    symlink(directory.path(), &alias).unwrap();
    for root in [
        alias.join("."),
        alias.join(".").join("."),
        alias.join("..").join(directory.path().file_name().unwrap()),
    ] {
        assert!(checked_root(&root).unwrap_err().contains("symlink"));
        assert!(check(&root).unwrap_err().contains("symlink"));
    }
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
    for target in ["local/note.md", "local/./note.md", "local/../README.md"] {
        write(
            directory.path(),
            "links.md",
            &format!("[local]({target})\n"),
        );
        assert!(check(directory.path()).unwrap_err().contains("symlink"));
    }
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
