// SPDX-License-Identifier: AGPL-3.0-only
//! Offline, opt-in adoption into an existing Git working-tree root.
//!
//! Planning is read-only. Applying rechecks the complete plan before staging
//! writes, then replaces each changed file atomically. Multiple replacements
//! are not a filesystem transaction; an I/O failure can leave a partial install.
use crate::{Result, guide};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime},
};

const BEGIN: &str = "<!-- JUSTIN-GUIDEBOOK:BEGIN -->";
const END: &str = "<!-- JUSTIN-GUIDEBOOK:END -->";
const IGNORE_BEGIN: &str = "# JUSTIN-GUIDEBOOK-PRIVATE:BEGIN";
const IGNORE_END: &str = "# JUSTIN-GUIDEBOOK-PRIVATE:END";
const GUIDE: &str = ".ai-guidebook/GUIDEBOOK.md";
const LOCK: &str = ".ai-guidebook/policy-lock.json";
const PRIVATE_PATTERNS: &str = "/PRIVATE.local.md\n/CLAUDE.local.md\n/private/\n/.private/\n/.local/\n/.env\n/.env.*\n!/.env.example";

#[derive(Clone, Debug)]
pub struct AdoptionPlan {
    pub target: PathBuf,
    pub files: BTreeMap<String, String>,
    pub preserved_adapters: Vec<String>,
    source_root: PathBuf,
    checked_target: PathBuf,
    checked_files: BTreeMap<String, String>,
    checked_adapters: Vec<String>,
    baseline: BTreeMap<String, Option<Vec<u8>>>,
    git_identity: GitIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileIdentity {
    created: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl FileIdentity {
    fn read(path: &Path) -> Result<Self> {
        let metadata = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            created: metadata.created().ok(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GitIdentity {
    target: FileIdentity,
    marker: FileIdentity,
    marker_contents: Option<Vec<u8>>,
}

impl GitIdentity {
    fn read(target: &Path) -> Result<Self> {
        let marker = target.join(".git");
        let marker_type = metadata(&marker)?.ok_or("Target has no Git working-tree marker")?;
        if marker_type.file_type().is_symlink() || !(marker_type.is_dir() || marker_type.is_file())
        {
            return Err(
                "Target Git working-tree marker must be a regular file or directory".into(),
            );
        }
        Ok(Self {
            target: FileIdentity::read(target)?,
            marker: FileIdentity::read(&marker)?,
            marker_contents: if marker_type.is_file() {
                Some(fs::read(marker).map_err(|e| e.to_string())?)
            } else {
                None
            },
        })
    }
}

fn metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

fn no_symlink_ancestors(path: &Path) -> Result<()> {
    // Inspect lexical prefixes so an `alias/.` suffix cannot hide a link.
    let mut ancestor = PathBuf::new();
    for component in path.components() {
        ancestor.push(component);
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        if let Some(value) = metadata(&ancestor)? {
            if value.file_type().is_symlink() {
                return Err(format!(
                    "Refusing a target with a symlink ancestor: {}",
                    ancestor.display()
                ));
            }
            if !value.is_dir() {
                return Err(format!(
                    "Target ancestor is not a directory: {}",
                    ancestor.display()
                ));
            }
        }
    }
    Ok(())
}

fn absolute_target(target: &Path) -> Result<PathBuf> {
    if target.as_os_str().is_empty()
        || target
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("Unsafe target path: empty or traversal path".into());
    }
    let expanded = if target.starts_with("~") {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .ok_or("Cannot expand target '~': home directory unavailable")?;
        PathBuf::from(home).join(target.strip_prefix("~").map_err(|e| e.to_string())?)
    } else {
        target.to_path_buf()
    };
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(expanded)
    };
    no_symlink_ancestors(&absolute)?;
    fs::canonicalize(&absolute).map_err(|e| format!("Target {}: {e}", absolute.display()))
}

fn git_root(target: &Path) -> Result<PathBuf> {
    run_git_root(target, crate::git_command(target), Duration::from_secs(10))
}

fn run_git_root(target: &Path, mut command: Command, timeout: Duration) -> Result<PathBuf> {
    let mut child = command
        .arg("-C")
        .arg(target)
        .args(["rev-parse", "--show-toplevel"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot inspect target Git working tree: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Timed out inspecting target Git working tree".into());
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Cannot inspect target Git working tree: {error}"));
            }
        }
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Target must be the root of an existing Git working tree".into());
    }
    let root = String::from_utf8(output.stdout).map_err(|e| format!("Invalid Git root: {e}"))?;
    let root = root
        .strip_suffix('\n')
        .unwrap_or(&root)
        .trim_end_matches('\r');
    fs::canonicalize(root).map_err(|e| format!("Invalid Git working-tree root: {e}"))
}

fn validate_target(root: &Path, target: &Path) -> Result<PathBuf> {
    let target = absolute_target(target)?;
    if target.starts_with(root) {
        return Err("Refusing adoption into the guidebook itself".into());
    }
    if git_root(&target)? != target {
        return Err("Target must be the root of an existing Git working tree".into());
    }
    Ok(target)
}

/// Validate all components, including dangling symlinks and non-directory parents.
fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let rel = Path::new(relative);
    if relative.is_empty()
        || relative.contains('\\')
        || rel.is_absolute()
        || !rel
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(format!("Unsafe relative path: {relative}"));
    }
    let mut path = root.to_path_buf();
    let count = rel.components().count();
    for (index, part) in rel.components().enumerate() {
        path.push(part);
        if let Some(value) = metadata(&path)? {
            if value.file_type().is_symlink() {
                return Err(format!("Refusing symlink: {relative}"));
            }
            if index + 1 < count && !value.is_dir() {
                return Err(format!("Parent is not a directory: {relative}"));
            }
            if index + 1 == count && !value.is_file() {
                return Err(format!("Target is not a regular file: {relative}"));
            }
        }
    }
    Ok(path)
}

fn bytes(root: &Path, relative: &str) -> Result<Option<Vec<u8>>> {
    let path = safe_path(root, relative)?;
    if metadata(&path)?.is_some() {
        fs::read(&path)
            .map(Some)
            .map_err(|e| format!("{relative}: {e}"))
    } else {
        Ok(None)
    }
}

fn text(baseline: &BTreeMap<String, Option<Vec<u8>>>, relative: &str) -> Result<String> {
    String::from_utf8(baseline[relative].clone().unwrap_or_default())
        .map_err(|e| format!("Existing {relative} must be UTF-8: {e}"))
}

fn managed(old: &str, body: &str, begin: &str, end: &str) -> Result<String> {
    let starts = old.matches(begin).count();
    let ends = old.matches(end).count();
    let block = format!("{begin}\n{}\n{end}", body.trim_end());
    if starts == 0 && ends == 0 {
        let separator = if old.is_empty() || old.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        return Ok(format!("{old}{separator}\n{block}\n"));
    }
    if starts != 1 || ends != 1 || old.find(begin) >= old.find(end) {
        return Err("Malformed or duplicate managed markers; refusing to overwrite".into());
    }
    let start = old.find(begin).ok_or("Missing managed opening marker")?;
    let finish = old.find(end).ok_or("Missing managed closing marker")? + end.len();
    Ok(format!("{}{block}{}", &old[..start], &old[finish..]))
}

pub fn plan(root: &Path, target: &Path) -> Result<AdoptionPlan> {
    let root = crate::security::checked_root(root)?;
    let target = validate_target(&root, target)?;
    let git_identity = GitIdentity::read(&target)?;
    crate::check_repo(
        &root,
        crate::policy_date_at(time::OffsetDateTime::now_utc())?,
    )?;
    let outputs = guide::outputs(&root)?;
    let expected_guide = outputs
        .get("GUIDEBOOK.md")
        .ok_or("Generated guide missing")?
        .clone();
    let lock: Value = serde_json::from_str(
        outputs
            .get("guidebook-lock.json")
            .ok_or("Generated guide lock missing")?,
    )
    .map_err(|e| e.to_string())?;
    let source_repo = lock
        .get("source_repo")
        .and_then(Value::as_str)
        .ok_or("Source lock has no managed source repository")?;
    let adapters = guide::adapters(GUIDE);
    // Every planned byte is derived from the same baseline used by apply. A
    // second read here could otherwise bless an edit made while planning.
    let mut baseline = BTreeMap::new();
    for relative in [GUIDE, LOCK, "AGENTS.md", ".gitignore"]
        .into_iter()
        .chain(adapters.keys().map(String::as_str))
    {
        baseline.insert(relative.to_owned(), bytes(&target, relative)?);
    }
    let existing_guide = baseline[GUIDE].as_ref();
    let existing_lock = baseline[LOCK].as_ref();
    match (existing_guide, existing_lock) {
        (None, None) => {}
        (Some(guide), Some(lock)) => {
            let old: Value = serde_json::from_slice(lock)
                .map_err(|e| format!("Invalid existing snapshot lock: {e}"))?;
            if old.get("source_repo").and_then(Value::as_str) != Some(source_repo) {
                return Err("Refusing an unmanaged snapshot".into());
            }
            let digest = format!("{:x}", Sha256::digest(guide));
            if old.get("guidebook_sha256").and_then(Value::as_str) != Some(digest.as_str()) {
                return Err("Existing snapshot was modified; review before updating".into());
            }
        }
        _ => return Err("Existing snapshot must include a matching guidebook and lock".into()),
    }
    let mut files = BTreeMap::from([
        (GUIDE.into(), expected_guide),
        (
            LOCK.into(),
            serde_json::to_string_pretty(&lock).map_err(|e| e.to_string())? + "\n",
        ),
        (
            "AGENTS.md".into(),
            managed(
                &text(&baseline, "AGENTS.md")?,
                &guide::bootstrap(GUIDE),
                BEGIN,
                END,
            )?,
        ),
        (
            ".gitignore".into(),
            managed(
                &text(&baseline, ".gitignore")?,
                PRIVATE_PATTERNS,
                IGNORE_BEGIN,
                IGNORE_END,
            )?,
        ),
    ]);
    let mut preserved_adapters = Vec::new();
    for (relative, body) in adapters {
        if baseline[&relative].is_some() {
            preserved_adapters.push(relative);
        } else {
            files.insert(relative, body);
        }
    }
    Ok(AdoptionPlan {
        checked_target: target.clone(),
        checked_files: files.clone(),
        checked_adapters: preserved_adapters.clone(),
        target,
        files,
        preserved_adapters,
        source_root: root,
        baseline,
        git_identity,
    })
}

pub fn apply(plan: &AdoptionPlan) -> Result<()> {
    if plan.target != plan.checked_target
        || plan.files != plan.checked_files
        || plan.preserved_adapters != plan.checked_adapters
    {
        return Err("Adoption plan changed after validation; create a fresh plan".into());
    }
    if validate_target(&plan.source_root, &plan.target)? != plan.checked_target {
        return Err("Adoption target changed after planning".into());
    }
    if GitIdentity::read(&plan.target)? != plan.git_identity {
        return Err("Git working-tree identity changed after planning; create a fresh plan".into());
    }
    for (relative, original) in &plan.baseline {
        if bytes(&plan.target, relative)? != *original {
            return Err(format!(
                "Target changed after planning: {relative}; create a fresh plan"
            ));
        }
    }
    // Stage every file first, so ordinary preflight or staging failures do not
    // overwrite existing instructions. NamedTempFile cleans up on every error.
    let mut staged = Vec::new();
    for (relative, content) in &plan.files {
        if plan.baseline[relative].as_deref() == Some(content.as_bytes()) {
            continue;
        }
        let destination = safe_path(&plan.target, relative)?;
        let parent = destination.parent().ok_or("No adoption output parent")?;
        fs::create_dir_all(parent).map_err(|e| format!("{relative}: {e}"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|e| format!("Cannot stage {relative}: {e}"))?;
        if let Some(old) = metadata(&destination)? {
            temporary
                .as_file()
                .set_permissions(old.permissions())
                .map_err(|e| e.to_string())?;
        }
        temporary
            .write_all(content.as_bytes())
            .map_err(|e| format!("Cannot stage {relative}: {e}"))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|e| format!("Cannot stage {relative}: {e}"))?;
        staged.push((relative, temporary));
    }
    for (written, (relative, temporary)) in staged.into_iter().enumerate() {
        let destination = safe_path(&plan.target, relative)?;
        if bytes(&plan.target, relative)? != plan.baseline[relative] {
            return Err(format!(
                "Target changed during apply: {relative}; {written} files already applied"
            ));
        }
        temporary.persist(&destination).map_err(|e| {
            format!("Cannot atomically replace {relative}; {written} files already applied: {e}")
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_git(root: &Path, args: &[&str]) {
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
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "Git fixture operation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    #[test]
    fn git_root_timeout_kills_and_reaps_child() {
        let scratch = tempfile::tempdir().unwrap();
        let helper_source = scratch.path().join("slow-git.rs");
        let slow_git = scratch.path().join("slow-git");
        let pid_file = scratch.path().join("child.pid");
        fs::write(
            &helper_source,
            r#"
use std::{env, fs, thread, time::Duration};

fn main() {
    let pid_file = env::var_os("PID_FILE").expect("PID_FILE");
    fs::write(pid_file, std::process::id().to_string()).expect("write child pid");
    thread::sleep(Duration::from_secs(5));
}
"#,
        )
        .unwrap();
        let compile = Command::new("rustc")
            .args(["--edition=2024"])
            .arg(&helper_source)
            .arg("-o")
            .arg(&slow_git)
            .output()
            .unwrap();
        assert!(
            compile.status.success(),
            "Rust slow-Git fixture failed: {}",
            String::from_utf8_lossy(&compile.stderr)
        );

        let target = scratch.path().join("target");
        fs::create_dir(&target).unwrap();
        let sentinel = target.join("preserve.txt");
        let original = b"unchanged".to_vec();
        fs::write(&sentinel, &original).unwrap();
        let mut command = Command::new(&slow_git);
        command.env("PID_FILE", &pid_file);
        let started = Instant::now();
        let result = run_git_root(&target, command, Duration::from_millis(500));
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_secs(3),
            "timed Git child was not terminated promptly: {elapsed:?}"
        );
        assert_eq!(
            result.unwrap_err(),
            "Timed out inspecting target Git working tree"
        );
        assert_eq!(
            fs::read(&sentinel).unwrap(),
            original,
            "timed-out Git child changed the adoption target"
        );

        let pid = fs::read_to_string(pid_file)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap();
        let pid_arg = pid.to_string();
        let probe = Command::new("kill")
            .args(["-0", &pid_arg])
            .status()
            .unwrap();
        assert!(!probe.success(), "timed-out Git child {pid} remains alive");
    }

    #[test]
    fn git_root_ignores_ambient_repository_redirects() {
        if let Some(target) = std::env::var_os("TOE_TICKLING_ADOPT_TEST_TARGET") {
            let target = PathBuf::from(target);
            let expected =
                PathBuf::from(std::env::var_os("TOE_TICKLING_ADOPT_TEST_EXPECTED").unwrap());
            if std::env::var_os("TOE_TICKLING_ADOPT_TEST_VALIDATE_CONFIG").is_some() {
                let source =
                    PathBuf::from(std::env::var_os("TOE_TICKLING_ADOPT_TEST_SOURCE").unwrap());
                assert_eq!(git_root(&target).unwrap(), expected);
                assert_eq!(validate_target(&source, &target).unwrap(), target);
            } else {
                assert_eq!(git_root(&target).unwrap(), expected);
            }
            if std::env::var_os("TOE_TICKLING_ADOPT_TEST_PRESERVE_CONFIG").is_some() {
                let output = crate::git_command(&target)
                    .args(["config", "--get", "user.name"])
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "documented Git config was discarded"
                );
                assert_eq!(
                    String::from_utf8(output.stdout).unwrap().trim(),
                    "Preserved Fixture User"
                );
            }
            return;
        }

        let scratch = tempfile::tempdir().unwrap();
        let source = scratch.path().join("source");
        let target = scratch.path().join("target");
        let linked = scratch.path().join("linked-target");
        let foreign = scratch.path().join("foreign");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        fs::create_dir(&foreign).unwrap();
        isolated_git(&target, &["init", "-q"]);
        isolated_git(
            &target,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "fixture",
            ],
        );
        let linked_path = linked.to_string_lossy().into_owned();
        isolated_git(
            &target,
            &["worktree", "add", "--detach", &linked_path, "HEAD"],
        );
        assert!(
            linked.join(".git").is_file(),
            "fixture is not a linked worktree"
        );
        let ambiguous = scratch.path().join("ambiguous-target");
        fs::create_dir(&ambiguous).unwrap();
        fs::write(
            ambiguous.join(".git"),
            fs::read(linked.join(".git")).unwrap(),
        )
        .unwrap();
        assert!(
            validate_target(&source, &ambiguous)
                .is_err_and(|error| error.contains("Target must be the root")),
            "ambiguous linked-worktree marker was not rejected"
        );
        isolated_git(&foreign, &["init", "-q"]);

        for redirect in ["git-dir", "work-tree", "common-dir", "index-file"] {
            let mut child = Command::new(std::env::current_exe().unwrap());
            child
                .args([
                    "--exact",
                    "adopt::tests::git_root_ignores_ambient_repository_redirects",
                    "--nocapture",
                ])
                .env("TOE_TICKLING_ADOPT_TEST_TARGET", &linked)
                .env("TOE_TICKLING_ADOPT_TEST_EXPECTED", &linked)
                .env_remove("TOE_TICKLING_ADOPT_TEST_VALIDATE_CONFIG")
                .env_remove("TOE_TICKLING_ADOPT_TEST_PRESERVE_CONFIG")
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
                        .env("GIT_DIR", foreign.join(".git"))
                        .env("GIT_WORK_TREE", &foreign);
                }
                "work-tree" => {
                    child.env("GIT_WORK_TREE", &foreign);
                }
                "common-dir" => {
                    child.env("GIT_COMMON_DIR", foreign.join(".git"));
                }
                "index-file" => {
                    child.env("GIT_INDEX_FILE", foreign.join(".git/index"));
                }
                _ => unreachable!(),
            }
            let output = child.output().unwrap();
            assert!(
                output.status.success(),
                "{redirect} Git-root child failed:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "adopt::tests::git_root_ignores_ambient_repository_redirects",
                "--nocapture",
            ])
            .env("TOE_TICKLING_ADOPT_TEST_TARGET", &linked)
            .env("TOE_TICKLING_ADOPT_TEST_EXPECTED", &linked)
            .env("TOE_TICKLING_ADOPT_TEST_VALIDATE_CONFIG", "1")
            .env("TOE_TICKLING_ADOPT_TEST_SOURCE", &source)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "core.worktree")
            .env("GIT_CONFIG_VALUE_0", &foreign)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "config redirect child failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "adopt::tests::git_root_ignores_ambient_repository_redirects",
                "--nocapture",
            ])
            .env("TOE_TICKLING_ADOPT_TEST_TARGET", &linked)
            .env("TOE_TICKLING_ADOPT_TEST_EXPECTED", &linked)
            .env("TOE_TICKLING_ADOPT_TEST_PRESERVE_CONFIG", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "user.name")
            .env("GIT_CONFIG_VALUE_0", "Preserved Fixture User")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "preserved config child failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
