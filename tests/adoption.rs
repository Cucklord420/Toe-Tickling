// SPDX-License-Identifier: AGPL-3.0-only
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;
use toe_tickling::{adopt, guide};

const GUIDE: &str = ".ai-guidebook/GUIDEBOOK.md";
const LOCK: &str = ".ai-guidebook/policy-lock.json";

struct Fixture {
    _work: TempDir,
    source: PathBuf,
    target: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let work = tempfile::Builder::new()
            .prefix("Rust adoption tests ")
            .tempdir()
            .unwrap();
        let source = work.path().join("guide source with spaces");
        let target = work.path().join("existing project with spaces");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        copy_public(Path::new(env!("CARGO_MANIFEST_DIR")), &source);
        guide::build(&source).unwrap();
        refresh_manifest(&source);
        git_init(&target);
        Self {
            _work: work,
            source,
            target,
        }
    }

    fn plan(&self) -> adopt::AdoptionPlan {
        adopt::plan(&self.source, &self.target).unwrap()
    }

    fn install(&self) {
        adopt::apply(&self.plan()).unwrap();
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.target.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn text(&self, relative: &str) -> String {
        fs::read_to_string(self.target.join(relative)).unwrap()
    }
}

fn copy_public(source: &Path, destination: &Path) {
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if [".git", "target", "private", "local-data"]
            .iter()
            .any(|name| entry.file_name() == *name)
        {
            continue;
        }
        let path = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&path).unwrap();
            copy_public(&entry.path(), &path);
        } else {
            fs::copy(entry.path(), path).unwrap();
        }
    }
}

fn refresh_manifest(root: &Path) {
    toe_tickling::write_json(
        &root.join("policy/reading-manifest.json"),
        &toe_tickling::current_manifest(root).unwrap(),
    )
    .unwrap();
}

fn git_init(path: &Path) {
    let result = Command::new("git")
        .args(["init", "-q"])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, here: &Path, output: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(here).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == ".git" {
                continue;
            }
            let path = entry.path();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                visit(root, &path, output);
            } else if kind.is_file() {
                output.insert(
                    path.strip_prefix(root).unwrap().to_string_lossy().into(),
                    fs::read(path).unwrap(),
                );
            } else if kind.is_symlink() {
                output.insert(
                    path.strip_prefix(root).unwrap().to_string_lossy().into(),
                    fs::read_link(path)
                        .unwrap()
                        .to_string_lossy()
                        .as_bytes()
                        .to_vec(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[test]
fn preview_is_read_only_and_contains_full_snapshot() {
    let f = Fixture::new();
    f.write("AGENTS.md", "Original local requirements.\n");
    let before = snapshot(&f.target);
    let plan = f.plan();
    assert_eq!(before, snapshot(&f.target));
    assert!(!f.target.join(".ai-guidebook").exists());
    assert_eq!(plan.files.len(), 8);
    assert_eq!(
        plan.files[GUIDE],
        fs::read_to_string(f.source.join("GUIDEBOOK.md")).unwrap()
    );
    assert!(plan.files[GUIDE].contains("AUD-020"));
    assert!(plan.files["AGENTS.md"].contains("MUST be Rust"));
}

#[test]
fn apply_installs_matching_snapshot_and_all_model_entry_points() {
    let f = Fixture::new();
    f.install();
    let lock: Value = serde_json::from_str(&f.text(LOCK)).unwrap();
    let digest = format!(
        "{:x}",
        Sha256::digest(fs::read(f.target.join(GUIDE)).unwrap())
    );
    assert_eq!(lock["guidebook_sha256"], digest);
    assert_eq!(
        fs::read(f.target.join(GUIDE)).unwrap(),
        fs::read(f.source.join("GUIDEBOOK.md")).unwrap()
    );
    for relative in [
        "AGENTS.md",
        ".gitignore",
        "CLAUDE.md",
        "GEMINI.md",
        ".github/copilot-instructions.md",
        ".cursor/rules/justin-guidebook.mdc",
    ] {
        assert!(f.target.join(relative).is_file(), "Missing {relative}");
    }
    let head = Command::new("git")
        .arg("-C")
        .arg(&f.target)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .unwrap();
    assert!(!head.status.success(), "Adoption must not create a commit");
}

#[test]
fn managed_blocks_preserve_exact_project_prefix_and_suffix() {
    let f = Fixture::new();
    let prefix = "# Existing project\r\nKeep local requirements.\r\n";
    let suffix = "\r\n# Local footer\r\n";
    f.write(
        "AGENTS.md",
        &format!(
            "{prefix}<!-- JUSTIN-GUIDEBOOK:BEGIN -->\nold\n<!-- JUSTIN-GUIDEBOOK:END -->{suffix}"
        ),
    );
    f.write(".gitignore", "build/\n*.generated");
    f.install();
    let agent = f.text("AGENTS.md");
    assert!(agent.starts_with(prefix));
    assert!(agent.ends_with(suffix));
    assert!(!agent.contains("\nold\n"));
    assert!(f.text(".gitignore").starts_with("build/\n*.generated\n"));
    assert!(f.text(".gitignore").contains("!/.env.example"));
}

#[test]
fn repeat_install_is_byte_identical() {
    let f = Fixture::new();
    f.install();
    let before = snapshot(&f.target);
    let next = f.plan();
    assert_eq!(next.preserved_adapters.len(), 4);
    adopt::apply(&next).unwrap();
    assert_eq!(before, snapshot(&f.target));
    assert_eq!(
        f.text("AGENTS.md")
            .matches("<!-- JUSTIN-GUIDEBOOK:BEGIN -->")
            .count(),
        1
    );
    assert_eq!(
        f.text(".gitignore")
            .matches("# JUSTIN-GUIDEBOOK-PRIVATE:BEGIN")
            .count(),
        1
    );
}

#[test]
fn existing_model_adapters_are_preserved_including_non_utf8_content() {
    let f = Fixture::new();
    let originals = guide::adapters(GUIDE);
    for relative in originals.keys() {
        f.write(relative, "Existing model-specific rules.\n");
    }
    fs::write(f.target.join("CLAUDE.md"), [0xff, 0xfe]).unwrap();
    let before = snapshot(&f.target);
    let plan = f.plan();
    assert_eq!(plan.preserved_adapters.len(), 4);
    for relative in originals.keys() {
        assert!(!plan.files.contains_key(relative));
    }
    adopt::apply(&plan).unwrap();
    for relative in originals.keys() {
        assert_eq!(fs::read(f.target.join(relative)).unwrap(), before[relative]);
    }
}

#[test]
fn malformed_or_duplicate_markers_refuse_the_entire_install() {
    let f = Fixture::new();
    for invalid in [
        "<!-- JUSTIN-GUIDEBOOK:BEGIN -->\n",
        "<!-- JUSTIN-GUIDEBOOK:END -->\n",
        "<!-- JUSTIN-GUIDEBOOK:END -->\n<!-- JUSTIN-GUIDEBOOK:BEGIN -->\n",
        "<!-- JUSTIN-GUIDEBOOK:BEGIN --><!-- JUSTIN-GUIDEBOOK:BEGIN --><!-- JUSTIN-GUIDEBOOK:END -->",
    ] {
        f.write("AGENTS.md", invalid);
        let before = snapshot(&f.target);
        assert!(
            adopt::plan(&f.source, &f.target)
                .unwrap_err()
                .contains("markers")
        );
        assert_eq!(before, snapshot(&f.target));
    }
    f.write("AGENTS.md", "Good existing project instructions.\n");
    f.write(".gitignore", "# JUSTIN-GUIDEBOOK-PRIVATE:BEGIN\n");
    assert!(adopt::plan(&f.source, &f.target).is_err());
    assert!(!f.target.join(".ai-guidebook").exists());
}

#[test]
fn invalid_utf8_managed_instructions_are_not_overwritten() {
    let f = Fixture::new();
    fs::write(f.target.join("AGENTS.md"), [0xff]).unwrap();
    let before = snapshot(&f.target);
    assert!(
        adopt::plan(&f.source, &f.target)
            .unwrap_err()
            .contains("UTF-8")
    );
    assert_eq!(before, snapshot(&f.target));
}

#[test]
fn modified_snapshot_and_invalid_or_foreign_lock_are_rejected() {
    let f = Fixture::new();
    f.install();
    let original_guide = f.text(GUIDE);
    let original_lock = f.text(LOCK);
    f.write(GUIDE, "Locally modified snapshot.\n");
    let before = snapshot(&f.target);
    assert!(
        adopt::plan(&f.source, &f.target)
            .unwrap_err()
            .contains("modified")
    );
    assert_eq!(before, snapshot(&f.target));
    f.write(GUIDE, &original_guide);
    f.write(LOCK, "{invalid lock");
    assert!(
        adopt::plan(&f.source, &f.target)
            .unwrap_err()
            .contains("lock")
    );
    let mut foreign: Value = serde_json::from_str(&original_lock).unwrap();
    foreign["source_repo"] = json!("https://example.invalid/unmanaged");
    f.write(LOCK, &serde_json::to_string(&foreign).unwrap());
    assert!(
        adopt::plan(&f.source, &f.target)
            .unwrap_err()
            .contains("unmanaged")
    );
}

#[test]
fn incomplete_snapshot_pairs_are_never_replaced() {
    let f = Fixture::new();
    f.write(GUIDE, "User-owned snapshot.\n");
    let before = snapshot(&f.target);
    assert!(adopt::plan(&f.source, &f.target).is_err());
    assert_eq!(before, snapshot(&f.target));
    fs::remove_file(f.target.join(GUIDE)).unwrap();
    f.write(LOCK, "{}\n");
    assert!(adopt::plan(&f.source, &f.target).is_err());
    assert!(!f.target.join("AGENTS.md").exists());
}

#[test]
fn source_update_replaces_managed_snapshot_and_preserves_project_rules() {
    let f = Fixture::new();
    f.write("AGENTS.md", "Original project policy.\n");
    f.install();
    let before = f.text(GUIDE);
    let path = f.source.join("policy.json");
    let mut policy: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    policy["policy_version"] = json!("1.1.99");
    fs::write(path, serde_json::to_vec_pretty(&policy).unwrap()).unwrap();
    guide::build(&f.source).unwrap();
    refresh_manifest(&f.source);
    f.install();
    assert_ne!(before, f.text(GUIDE));
    assert!(
        f.text("AGENTS.md")
            .starts_with("Original project policy.\n")
    );
}

#[test]
fn stale_source_is_rejected_before_target_changes() {
    let f = Fixture::new();
    fs::write(f.source.join("GUIDEBOOK.md"), "stale generated guide\n").unwrap();
    let before = snapshot(&f.target);
    assert!(
        adopt::plan(&f.source, &f.target)
            .unwrap_err()
            .contains("stale")
    );
    assert_eq!(before, snapshot(&f.target));
}

#[test]
fn source_changes_after_preview_do_not_change_pinned_plan_bytes() {
    let f = Fixture::new();
    let plan = f.plan();
    let guide = plan.files[GUIDE].clone();
    fs::write(f.source.join("GUIDEBOOK.md"), "Changed after preview\n").unwrap();
    adopt::apply(&plan).unwrap();
    assert_eq!(f.text(GUIDE), guide);
}

#[test]
fn non_git_missing_and_nested_targets_are_rejected_without_creating_files() {
    let f = Fixture::new();
    let missing = f._work.path().join("missing target");
    assert!(adopt::plan(&f.source, &missing).is_err());
    assert!(!missing.exists());
    let outside = f._work.path().join("not a repository");
    fs::create_dir(&outside).unwrap();
    assert!(adopt::plan(&f.source, &outside).is_err());
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    let child = f.target.join("child");
    fs::create_dir(&child).unwrap();
    assert!(adopt::plan(&f.source, &child).is_err());
    assert_eq!(fs::read_dir(&child).unwrap().count(), 0);
    assert!(adopt::plan(&f.source, Path::new("")).is_err());
}

#[test]
fn self_adoption_and_targets_inside_guidebook_are_rejected() {
    let f = Fixture::new();
    git_init(&f.source);
    assert!(
        adopt::plan(&f.source, &f.source)
            .unwrap_err()
            .contains("guidebook itself")
    );
    let child = f.source.join("nested project");
    fs::create_dir(&child).unwrap();
    git_init(&child);
    assert!(
        adopt::plan(&f.source, &child)
            .unwrap_err()
            .contains("guidebook itself")
    );
}

#[test]
fn target_path_traversal_and_public_plan_mutations_are_rejected() {
    let f = Fixture::new();
    let traversing = f.target.join("..").join("existing project with spaces");
    assert!(
        adopt::plan(&f.source, &traversing)
            .unwrap_err()
            .contains("Unsafe")
    );
    let mut plan = f.plan();
    plan.files
        .insert("../outside.md".into(), "Must not be written".into());
    assert!(
        adopt::apply(&plan)
            .unwrap_err()
            .contains("changed after validation")
    );
    assert!(!f._work.path().join("outside.md").exists());
    let mut plan = f.plan();
    plan.target = f.source.clone();
    assert!(adopt::apply(&plan).is_err());
    assert!(!f.target.join("AGENTS.md").exists());
}

#[test]
fn non_directory_parents_and_file_type_collisions_block_all_writes() {
    let f = Fixture::new();
    for path in [".github", ".cursor", ".ai-guidebook"] {
        f.write(path, "Not a directory\n");
        let before = snapshot(&f.target);
        assert!(
            adopt::plan(&f.source, &f.target)
                .unwrap_err()
                .contains("directory")
        );
        assert_eq!(before, snapshot(&f.target));
        fs::remove_file(f.target.join(path)).unwrap();
    }
    fs::create_dir(f.target.join("CLAUDE.md")).unwrap();
    assert!(
        adopt::plan(&f.source, &f.target)
            .unwrap_err()
            .contains("regular file")
    );
    assert!(!f.target.join("AGENTS.md").exists());
}

#[test]
fn concurrent_project_edits_and_new_adapters_invalidate_plan_before_writes() {
    let f = Fixture::new();
    f.write("AGENTS.md", "Before planning.\n");
    let plan = f.plan();
    f.write("AGENTS.md", "Another worker's requirements.\n");
    let before = snapshot(&f.target);
    assert!(
        adopt::apply(&plan)
            .unwrap_err()
            .contains("changed after planning")
    );
    assert_eq!(before, snapshot(&f.target));
    let plan = f.plan();
    f.write("CLAUDE.md", "Another worker's adapter.\n");
    let before = snapshot(&f.target);
    assert!(adopt::apply(&plan).is_err());
    assert_eq!(before, snapshot(&f.target));
    let plan = f.plan();
    f.write("CLAUDE.md", "Modified preserved adapter.\n");
    assert!(adopt::apply(&plan).is_err());
    assert!(!f.target.join(".ai-guidebook").exists());
}

#[test]
fn snapshot_tampering_after_preview_blocks_apply() {
    let f = Fixture::new();
    f.install();
    let plan = f.plan();
    f.write(GUIDE, "Tampered after planning\n");
    let before = snapshot(&f.target);
    assert!(adopt::apply(&plan).is_err());
    assert_eq!(before, snapshot(&f.target));
}

#[test]
fn disappearing_git_identity_after_preview_blocks_apply() {
    let f = Fixture::new();
    let plan = f.plan();
    fs::rename(
        f.target.join(".git"),
        f._work.path().join("saved git directory"),
    )
    .unwrap();
    assert!(adopt::apply(&plan).is_err());
    assert!(!f.target.join("AGENTS.md").exists());
}

#[test]
fn replacement_repository_at_the_same_path_invalidates_preview() {
    let f = Fixture::new();
    let plan = f.plan();
    fs::rename(&f.target, f._work.path().join("original project")).unwrap();
    fs::create_dir(&f.target).unwrap();
    git_init(&f.target);
    assert!(
        adopt::apply(&plan)
            .unwrap_err()
            .contains("identity changed")
    );
    assert!(!f.target.join("AGENTS.md").exists());
}

#[test]
fn full_source_privacy_and_manifest_checks_gate_adoption() {
    let f = Fixture::new();
    fs::write(
        f.source.join("extra.md"),
        format!("{}{}", "ghp_", "A".repeat(30)),
    )
    .unwrap();
    assert!(
        adopt::plan(&f.source, &f.target)
            .unwrap_err()
            .contains("credential")
    );
    fs::remove_file(f.source.join("extra.md")).unwrap();
    fs::write(
        f.source.join("README.md"),
        "Changed since the recorded reading manifest.\n",
    )
    .unwrap();
    assert!(
        adopt::plan(&f.source, &f.target)
            .unwrap_err()
            .contains("manifest")
    );
    assert!(!f.target.join("AGENTS.md").exists());
}

#[cfg(unix)]
#[test]
fn symlink_targets_ancestors_and_managed_paths_are_rejected() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let alias = f._work.path().join("alias");
    symlink(&f.target, &alias).unwrap();
    assert!(
        adopt::plan(&f.source, &alias)
            .unwrap_err()
            .contains("symlink")
    );
    let nested = f.target.join("nested Git root");
    fs::create_dir(&nested).unwrap();
    git_init(&nested);
    assert!(
        adopt::plan(&f.source, &alias.join("nested Git root"))
            .unwrap_err()
            .contains("symlink")
    );
    let outside = f._work.path().join("outside.txt");
    fs::write(&outside, "Outside file remains untouched.\n").unwrap();
    for relative in ["AGENTS.md", "CLAUDE.md", ".gitignore"] {
        symlink(&outside, f.target.join(relative)).unwrap();
        assert!(
            adopt::plan(&f.source, &f.target)
                .unwrap_err()
                .contains("symlink")
        );
        fs::remove_file(f.target.join(relative)).unwrap();
    }
    let outside_dir = f._work.path().join("outside directory");
    fs::create_dir(&outside_dir).unwrap();
    for relative in [".ai-guidebook", ".github", ".cursor"] {
        symlink(&outside_dir, f.target.join(relative)).unwrap();
        assert!(
            adopt::plan(&f.source, &f.target)
                .unwrap_err()
                .contains("symlink")
        );
        fs::remove_file(f.target.join(relative)).unwrap();
    }
    symlink(
        f._work.path().join("nonexistent"),
        f.target.join("GEMINI.md"),
    )
    .unwrap();
    assert!(
        adopt::plan(&f.source, &f.target)
            .unwrap_err()
            .contains("symlink")
    );
    assert_eq!(
        fs::read_to_string(outside).unwrap(),
        "Outside file remains untouched.\n"
    );
    assert_eq!(fs::read_dir(outside_dir).unwrap().count(), 0);
    assert!(!f.target.join("AGENTS.md").exists());
}

#[cfg(unix)]
#[test]
fn symlinks_introduced_after_preview_are_rejected() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let plan = f.plan();
    let outside = f._work.path().join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, f.target.join(".ai-guidebook")).unwrap();
    assert!(adopt::apply(&plan).unwrap_err().contains("symlink"));
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    assert!(!f.target.join("AGENTS.md").exists());
}

#[cfg(unix)]
#[test]
fn replacing_existing_instructions_preserves_their_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.write("AGENTS.md", "Existing project instructions.\n");
    let path = f.target.join("AGENTS.md");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    f.install();
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}
