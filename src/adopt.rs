// SPDX-License-Identifier: AGPL-3.0-only
//! Offline, opt-in adoption into an existing Git working-tree root.
//!
//! Planning is read-only. Applying records a private recovery journal before
//! staging writes, then replaces each changed file atomically. Multiple
//! replacements are not a filesystem transaction; use `adopt TARGET --recover`
//! after an interruption to restore unchanged installer outputs safely.
use crate::{Result, guide};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BEGIN: &str = "<!-- JUSTIN-GUIDEBOOK:BEGIN -->";
const END: &str = "<!-- JUSTIN-GUIDEBOOK:END -->";
const IGNORE_BEGIN: &str = "# JUSTIN-GUIDEBOOK-PRIVATE:BEGIN";
const IGNORE_END: &str = "# JUSTIN-GUIDEBOOK-PRIVATE:END";
const GUIDE: &str = ".ai-guidebook/GUIDEBOOK.md";
const LOCK: &str = ".ai-guidebook/policy-lock.json";
const PRIVATE_PATTERNS: &str = "/PRIVATE.local.md\n/CLAUDE.local.md\n/private/\n/.private/\n/.local/\n/.env\n/.env.*\n!/.env.example";
// Keep the location stable so pending v1 journals remain discoverable.
const JOURNAL_NAME: &str = ".toe-tickling-adoption-v1.json";
const LEGACY_JOURNAL_VERSION: u32 = 1;
const LEGACY_JOURNAL_V2_VERSION: u32 = 2;
const JOURNAL_VERSION: u32 = 3;
static NEXT_TRANSACTION: AtomicU64 = AtomicU64::new(0);
type AfterPersistHook = dyn FnMut(usize, &str) -> Result<()>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct JournalPermissions {
    readonly: bool,
    #[serde(default)]
    unix_mode: Option<u32>,
}

impl JournalPermissions {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = metadata.permissions().mode() & 0o7777;
            Self {
                readonly: mode & 0o222 == 0,
                unix_mode: Some(mode),
            }
        }
        #[cfg(not(unix))]
        {
            Self {
                readonly: metadata.permissions().readonly(),
                unix_mode: None,
            }
        }
    }

    fn default_new_file() -> Self {
        #[cfg(unix)]
        {
            Self {
                readonly: false,
                unix_mode: Some(0o600),
            }
        }
        #[cfg(not(unix))]
        {
            Self {
                readonly: false,
                unix_mode: None,
            }
        }
    }

    fn validate(&self) -> Result<()> {
        #[cfg(unix)]
        {
            let mode = self
                .unix_mode
                .ok_or("Journal is missing Unix permissions")?;
            if mode > 0o7777 || self.readonly != (mode & 0o222 == 0) {
                return Err("Journal contains invalid Unix permissions".into());
            }
        }
        #[cfg(not(unix))]
        if self.unix_mode.is_some() {
            return Err("Journal permissions do not match this platform".into());
        }
        Ok(())
    }

    fn apply_to(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let mut permissions = fs::metadata(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(
                self.unix_mode
                    .ok_or("Journal is missing Unix permissions")?,
            );
        }
        #[cfg(not(unix))]
        permissions.set_readonly(self.readonly);
        fs::set_permissions(path, permissions).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct JournalFileState {
    bytes: Option<Vec<u8>>,
    permissions: Option<JournalPermissions>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum JournalState {
    Prepared,
    Committed,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum JournalTransition {
    #[default]
    Stable,
    Pending,
    AttributesCleared,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct JournalIdentity {
    target: String,
    git_dir: String,
    git_identity: String,
    source_repo: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct JournalEntry {
    relative: String,
    before: JournalFileState,
    after: JournalFileState,
    stage: String,
    rollback_stage: String,
    #[serde(default)]
    transition: JournalTransition,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct AdoptionJournal {
    version: u32,
    transaction: String,
    state: JournalState,
    identity: JournalIdentity,
    #[serde(default)]
    planned_paths: Vec<String>,
    #[serde(default)]
    integrity_sha256: String,
    entries: Vec<JournalEntry>,
}

#[derive(Serialize)]
struct AdoptionJournalV2<'a> {
    version: u32,
    transaction: &'a str,
    state: JournalState,
    identity: &'a JournalIdentity,
    planned_paths: &'a [String],
    integrity_sha256: &'a str,
    entries: Vec<JournalEntryV2<'a>>,
}

#[derive(Serialize)]
struct JournalEntryV2<'a> {
    relative: &'a str,
    before: &'a JournalFileState,
    after: &'a JournalFileState,
    stage: &'a str,
    rollback_stage: &'a str,
}

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

fn run_git_root(target: &Path, command: Command, timeout: Duration) -> Result<PathBuf> {
    let root = run_git_path(
        target,
        command,
        &["rev-parse", "--show-toplevel"],
        timeout,
        "Timed out inspecting target Git working tree",
        "Target must be the root of an existing Git working tree",
    )?;
    fs::canonicalize(root).map_err(|e| format!("Invalid Git working-tree root: {e}"))
}

fn run_git_path(
    target: &Path,
    mut command: Command,
    args: &[&str],
    timeout: Duration,
    timeout_error: &str,
    failure_error: &str,
) -> Result<PathBuf> {
    let mut child = command
        .arg("-C")
        .arg(target)
        .args(args)
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
                return Err(timeout_error.into());
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
        return Err(failure_error.into());
    }
    let path = String::from_utf8(output.stdout).map_err(|e| format!("Invalid Git path: {e}"))?;
    let path = path
        .strip_suffix('\n')
        .unwrap_or(&path)
        .trim_end_matches('\r');
    if path.is_empty() {
        return Err("Git returned an empty path".into());
    }
    Ok(PathBuf::from(path))
}

fn git_directory(target: &Path) -> Result<PathBuf> {
    let command = crate::git_command(target);
    let reported = run_git_path(
        target,
        command,
        &["rev-parse", "--absolute-git-dir"],
        Duration::from_secs(10),
        "Timed out inspecting target Git metadata directory",
        "Cannot locate target Git metadata directory",
    )?;
    if !reported.is_absolute() {
        return Err("Git metadata directory must be absolute".into());
    }
    no_symlink_ancestors(&reported)?;
    fs::canonicalize(&reported).map_err(|e| format!("Invalid Git metadata directory: {e}"))
}

fn journal_path(target: &Path) -> Result<(PathBuf, PathBuf)> {
    let git_dir = git_directory(target)?;
    no_symlink_ancestors(&git_dir)?;
    let journal = git_dir.join(JOURNAL_NAME);
    if let Some(value) = metadata(&journal)?
        && (value.file_type().is_symlink() || !value.is_file())
    {
        return Err("Adoption recovery journal must be a regular file".into());
    }
    Ok((git_dir, journal))
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

fn git_identity_fingerprint(identity: &GitIdentity) -> String {
    let created = |value: Option<SystemTime>| {
        value
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos().to_string())
            .unwrap_or_else(|| "unavailable".into())
    };
    #[cfg(unix)]
    let file_ids = format!(
        ";target={}:{};marker={}:{}",
        identity.target.device,
        identity.target.inode,
        identity.marker.device,
        identity.marker.inode
    );
    #[cfg(not(unix))]
    let file_ids = String::new();
    let material = format!(
        "target-created={};marker-created={};marker-content={}",
        created(identity.target.created),
        created(identity.marker.created),
        identity
            .marker_contents
            .as_ref()
            .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
            .unwrap_or_else(|| "directory".into())
    ) + &file_ids;
    format!("{:x}", Sha256::digest(material.as_bytes()))
}

fn source_repo(root: &Path) -> Result<String> {
    let outputs = guide::outputs(root)?;
    let lock: Value = serde_json::from_str(
        outputs
            .get("guidebook-lock.json")
            .ok_or("Generated guide lock missing")?,
    )
    .map_err(|e| e.to_string())?;
    lock.get("source_repo")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "Source lock has no managed source repository".into())
}

fn output_paths() -> BTreeSet<String> {
    [GUIDE, LOCK, "AGENTS.md", ".gitignore"]
        .into_iter()
        .map(str::to_owned)
        .chain(guide::adapters(GUIDE).into_keys())
        .collect()
}

fn transaction_id(target: &Path, source: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = NEXT_TRANSACTION.fetch_add(1, Ordering::Relaxed);
    let material = format!(
        "{}:{source}:{now}:{}:{counter}",
        target.display(),
        std::process::id()
    );
    format!("{:x}", Sha256::digest(material.as_bytes()))
}

fn stage_relative(relative: &str, transaction: &str, rollback: bool) -> Result<String> {
    let path = Path::new(relative);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Managed output path must have a UTF-8 filename")?;
    let suffix = if rollback { "rollback" } else { "install" };
    let mut staged = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    staged.push(format!(".{name}.toe-tickling-{transaction}-{suffix}.stage"));
    staged
        .to_str()
        .map(|path| path.replace('\\', "/"))
        .ok_or_else(|| "Staging path must be UTF-8".into())
}

fn file_state(root: &Path, relative: &str) -> Result<JournalFileState> {
    let path = safe_path(root, relative)?;
    match metadata(&path)? {
        None => Ok(JournalFileState {
            bytes: None,
            permissions: None,
        }),
        Some(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(format!("Refusing non-regular adoption output: {relative}"));
            }
            Ok(JournalFileState {
                bytes: Some(fs::read(&path).map_err(|e| format!("{relative}: {e}"))?),
                permissions: Some(JournalPermissions::from_metadata(&metadata)),
            })
        }
    }
}

fn state_for_bytes(bytes: Vec<u8>, permissions: JournalPermissions) -> JournalFileState {
    JournalFileState {
        bytes: Some(bytes),
        permissions: Some(permissions),
    }
}

fn stage_is_safe_prefix(root: &Path, relative: &str, expected: &JournalFileState) -> Result<bool> {
    let Some(expected_bytes) = expected.bytes.as_deref() else {
        return Ok(false);
    };
    let actual = file_state(root, relative)?;
    let Some(actual_bytes) = actual.bytes.as_deref() else {
        return Ok(true);
    };
    Ok(actual_bytes.len() <= expected_bytes.len() && expected_bytes.starts_with(actual_bytes))
}

fn validate_state(state: &JournalFileState, label: &str) -> Result<()> {
    match (&state.bytes, &state.permissions) {
        (None, None) => Ok(()),
        (Some(_), Some(permissions)) => permissions
            .validate()
            .map_err(|error| format!("Invalid {label} permissions: {error}")),
        _ => Err(format!("Invalid {label} file state")),
    }
}

fn validate_snapshot_pair(
    guide_state: &JournalFileState,
    lock_state: &JournalFileState,
    source: &str,
    label: &str,
) -> Result<()> {
    match (guide_state.bytes.as_deref(), lock_state.bytes.as_deref()) {
        (None, None) => Ok(()),
        (Some(guide), Some(lock)) => {
            let lock: Value = serde_json::from_slice(lock)
                .map_err(|e| format!("Invalid {label} snapshot lock: {e}"))?;
            if lock.get("source_repo").and_then(Value::as_str) != Some(source) {
                return Err(format!(
                    "{label} snapshot belongs to another source repository"
                ));
            }
            let digest = format!("{:x}", Sha256::digest(guide));
            if lock.get("guidebook_sha256").and_then(Value::as_str) != Some(digest.as_str()) {
                return Err(format!("{label} guidebook and lock are inconsistent"));
            }
            Ok(())
        }
        _ => Err(format!(
            "{label} snapshot and lock must be present together"
        )),
    }
}

fn create_journal(target: &Path, root: &Path, plan: &AdoptionPlan) -> Result<AdoptionJournal> {
    let source = source_repo(root)?;
    let (git_dir, _) = journal_path(target)?;
    let identity = GitIdentity::read(target)?;
    let target_text = target
        .to_str()
        .ok_or("Canonical adoption target must be UTF-8")?
        .to_owned();
    let git_dir_text = git_dir
        .to_str()
        .ok_or("Git metadata directory must be UTF-8")?
        .to_owned();
    let transaction = transaction_id(target, &source);
    let mut entries = Vec::new();
    for (relative, content) in &plan.files {
        let before = file_state(target, relative)?;
        if before.bytes != plan.baseline[relative] {
            return Err(format!(
                "Target changed after planning: {relative}; create a fresh plan"
            ));
        }
        let permissions = before
            .permissions
            .clone()
            .unwrap_or_else(JournalPermissions::default_new_file);
        let stage = stage_relative(relative, &transaction, false)?;
        let rollback_stage = stage_relative(relative, &transaction, true)?;
        for candidate in [&stage, &rollback_stage] {
            let path = safe_path(target, candidate)?;
            if metadata(&path)?.is_some() {
                return Err(format!("Adoption staging path already exists: {candidate}"));
            }
        }
        entries.push(JournalEntry {
            relative: relative.clone(),
            before,
            after: state_for_bytes(content.as_bytes().to_vec(), permissions),
            stage,
            rollback_stage,
            transition: JournalTransition::Stable,
        });
    }
    if entries.iter().all(|entry| entry.before == entry.after) {
        return Err("No changed adoption outputs need a recovery journal".into());
    }
    let planned_paths = plan.files.keys().cloned().collect();
    let mut journal = AdoptionJournal {
        version: JOURNAL_VERSION,
        transaction,
        state: JournalState::Prepared,
        identity: JournalIdentity {
            target: target_text,
            git_dir: git_dir_text,
            git_identity: git_identity_fingerprint(&identity),
            source_repo: source,
        },
        planned_paths,
        integrity_sha256: String::new(),
        entries,
    };
    seal_journal(&mut journal)?;
    Ok(journal)
}

fn journal_integrity(journal: &AdoptionJournal) -> Result<String> {
    let bytes = if journal.version == LEGACY_JOURNAL_V2_VERSION {
        let legacy = AdoptionJournalV2 {
            version: journal.version,
            transaction: &journal.transaction,
            state: journal.state,
            identity: &journal.identity,
            planned_paths: &journal.planned_paths,
            integrity_sha256: "",
            entries: journal
                .entries
                .iter()
                .map(|entry| JournalEntryV2 {
                    relative: &entry.relative,
                    before: &entry.before,
                    after: &entry.after,
                    stage: &entry.stage,
                    rollback_stage: &entry.rollback_stage,
                })
                .collect(),
        };
        serde_json::to_vec(&legacy).map_err(|e| e.to_string())?
    } else {
        let mut unsigned = journal.clone();
        unsigned.integrity_sha256.clear();
        serde_json::to_vec(&unsigned).map_err(|e| e.to_string())?
    };
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn seal_journal(journal: &mut AdoptionJournal) -> Result<()> {
    journal.integrity_sha256 = journal_integrity(journal)?;
    Ok(())
}

fn validate_live_identity(
    source_root: &Path,
    target: &Path,
    identity: &JournalIdentity,
) -> Result<()> {
    if validate_target(source_root, target)? != target {
        return Err("Adoption target identity changed".into());
    }
    let (git_dir, _) = journal_path(target)?;
    if git_dir.to_str() != Some(identity.git_dir.as_str())
        || target.to_str() != Some(identity.target.as_str())
        || git_identity_fingerprint(&GitIdentity::read(target)?) != identity.git_identity
        || source_repo(source_root)? != identity.source_repo
    {
        return Err("Adoption source or Git worktree identity changed".into());
    }
    Ok(())
}

fn validate_journal(
    source_root: &Path,
    target: &Path,
    git_dir: &Path,
    journal: &AdoptionJournal,
) -> Result<()> {
    let legacy_v1 = journal.version == LEGACY_JOURNAL_VERSION
        && journal.planned_paths.is_empty()
        && journal.integrity_sha256.is_empty();
    let legacy_v2 = journal.version == LEGACY_JOURNAL_V2_VERSION;
    if !matches!(journal.version, JOURNAL_VERSION | LEGACY_JOURNAL_V2_VERSION) && !legacy_v1 {
        return Err(format!(
            "Unsupported adoption journal version: {}",
            journal.version
        ));
    }
    if (legacy_v1 || legacy_v2)
        && journal
            .entries
            .iter()
            .any(|entry| entry.transition != JournalTransition::Stable)
    {
        return Err(
            "Legacy adoption journals cannot authorize a destination permission transition".into(),
        );
    }
    if !legacy_v1 && journal.integrity_sha256 != journal_integrity(journal)? {
        return Err("Adoption recovery journal integrity check failed".into());
    }
    if journal.transaction.len() != 64
        || !journal
            .transaction
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("Invalid adoption journal transaction id".into());
    }
    validate_live_identity(source_root, target, &journal.identity)?;
    if git_dir.to_str() != Some(journal.identity.git_dir.as_str()) {
        return Err("Adoption journal belongs to another Git worktree".into());
    }
    if journal.entries.is_empty() {
        return Err("Adoption journal has no entries".into());
    }
    let entry_paths: Vec<_> = journal
        .entries
        .iter()
        .map(|entry| entry.relative.as_str())
        .collect();
    if !legacy_v1 {
        let planned_paths: Vec<_> = journal.planned_paths.iter().map(String::as_str).collect();
        if planned_paths != entry_paths {
            return Err("Adoption journal planned output list does not match its entries".into());
        }
    }
    if ![GUIDE, LOCK, "AGENTS.md", ".gitignore"]
        .iter()
        .all(|required| entry_paths.contains(required))
    {
        return Err("Adoption journal is missing a required managed output".into());
    }
    let allowed = output_paths();
    let mut seen = BTreeSet::new();
    let mut stages = BTreeSet::new();
    let mut by_path = BTreeMap::new();
    for entry in &journal.entries {
        if !allowed.contains(&entry.relative) || !seen.insert(entry.relative.as_str()) {
            return Err(format!(
                "Invalid or duplicate journal output path: {}",
                entry.relative
            ));
        }
        validate_state(&entry.before, "preimage")?;
        validate_state(&entry.after, "postimage")?;
        if entry.after.bytes.is_none() {
            return Err(format!("Journal postimage is missing: {}", entry.relative));
        }
        if entry.stage != stage_relative(&entry.relative, &journal.transaction, false)?
            || entry.rollback_stage != stage_relative(&entry.relative, &journal.transaction, true)?
            || !stages.insert(entry.stage.as_str())
            || !stages.insert(entry.rollback_stage.as_str())
            || allowed.contains(&entry.stage)
            || allowed.contains(&entry.rollback_stage)
        {
            return Err(format!("Invalid journal staging path: {}", entry.relative));
        }
        for staged in [&entry.stage, &entry.rollback_stage] {
            let staged_path = safe_path(target, staged)?;
            if metadata(&staged_path)?.is_some() {
                let expected = if staged == &entry.stage {
                    &entry.after
                } else {
                    &entry.before
                };
                if !stage_is_safe_prefix(target, staged, expected)? {
                    return Err(format!("Unrecognized adoption staging file: {staged}"));
                }
            }
        }
        by_path.insert(entry.relative.as_str(), entry);
    }
    let guide_entry = by_path
        .get(GUIDE)
        .ok_or("Journal is missing the guidebook output")?;
    let lock_entry = by_path
        .get(LOCK)
        .ok_or("Journal is missing the lock output")?;
    validate_snapshot_pair(
        &guide_entry.before,
        &lock_entry.before,
        &journal.identity.source_repo,
        "Original",
    )?;
    validate_snapshot_pair(
        &guide_entry.after,
        &lock_entry.after,
        &journal.identity.source_repo,
        "Installed",
    )?;
    Ok(())
}

fn write_journal_exclusive(path: &Path, journal: &AdoptionJournal) -> Result<()> {
    if metadata(path)?.is_some() {
        return Err(
            "Pending adoption recovery already exists; run `adopt TARGET --recover`".into(),
        );
    }
    let encoded = serde_json::to_vec_pretty(journal).map_err(|e| e.to_string())?;
    let parent = path
        .parent()
        .ok_or("Recovery journal has no parent directory")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("Cannot stage adoption recovery journal: {e}"))?;
    temporary
        .write_all(&encoded)
        .map_err(|e| format!("Cannot write adoption recovery journal: {e}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|e| format!("Cannot sync adoption recovery journal: {e}"))?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| format!("Cannot create adoption recovery journal: {}", error.error))?;
    Ok(())
}

fn write_journal_replacement(path: &Path, journal: &AdoptionJournal) -> Result<()> {
    let current = metadata(path)?.ok_or("Adoption recovery journal disappeared")?;
    if current.file_type().is_symlink() || !current.is_file() {
        return Err("Adoption recovery journal must be a regular file".into());
    }
    let encoded = serde_json::to_vec_pretty(journal).map_err(|e| e.to_string())?;
    let parent = path
        .parent()
        .ok_or("Recovery journal has no parent directory")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("Cannot stage adoption journal update: {e}"))?;
    temporary
        .write_all(&encoded)
        .map_err(|e| format!("Cannot write adoption journal update: {e}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|e| format!("Cannot sync adoption journal update: {e}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("Cannot replace adoption journal: {}", error.error))?;
    Ok(())
}

fn write_journal_update(
    path: &Path,
    expected_current: &AdoptionJournal,
    updated: &AdoptionJournal,
) -> Result<()> {
    if read_journal(path)? != *expected_current {
        return Err("Adoption recovery journal changed during an update".into());
    }
    write_journal_replacement(path, updated)
}

fn read_journal(path: &Path) -> Result<AdoptionJournal> {
    let value = metadata(path)?.ok_or("No pending adoption recovery journal")?;
    if value.file_type().is_symlink() || !value.is_file() {
        return Err("Adoption recovery journal must be a regular file".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("Cannot read adoption journal: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid adoption recovery journal: {e}"))
}

fn migrate_legacy_journal(path: &Path, journal: &AdoptionJournal) -> Result<AdoptionJournal> {
    if journal.version == JOURNAL_VERSION {
        return Ok(journal.clone());
    }
    let mut migrated = journal.clone();
    migrated.version = JOURNAL_VERSION;
    migrated.planned_paths = migrated
        .entries
        .iter()
        .map(|entry| entry.relative.clone())
        .collect();
    for entry in &mut migrated.entries {
        entry.transition = JournalTransition::Stable;
    }
    seal_journal(&mut migrated)?;
    write_journal_update(path, journal, &migrated)?;
    Ok(migrated)
}

fn ensure_parent_dirs(target: &Path, relative: &str) -> Result<()> {
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
    let mut current = target.to_path_buf();
    let mut components = rel.components().peekable();
    while let Some(component) = components.next() {
        current.push(component);
        if components.peek().is_none() {
            break;
        }
        match metadata(&current)? {
            Some(value) if value.file_type().is_symlink() => {
                return Err(format!("Refusing symlink parent: {relative}"));
            }
            Some(value) if !value.is_dir() => {
                return Err(format!("Parent is not a directory: {relative}"));
            }
            Some(_) => {}
            None => match fs::create_dir(&current) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let created = metadata(&current)?.ok_or("Created parent disappeared")?;
                    if created.file_type().is_symlink() || !created.is_dir() {
                        return Err(format!("Refusing unsafe parent: {relative}"));
                    }
                }
                Err(error) => return Err(format!("Cannot create parent for {relative}: {error}")),
            },
        }
    }
    safe_path(target, relative)?;
    Ok(())
}

fn create_stage(target: &Path, relative: &str, bytes: &[u8]) -> Result<()> {
    ensure_parent_dirs(target, relative)?;
    let path = safe_path(target, relative)?;
    if metadata(&path)?.is_some() {
        return Err(format!("Adoption staging path already exists: {relative}"));
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("Cannot create stage {relative}: {e}"))?;
    JournalPermissions::default_new_file().apply_to(&path)?;
    let mut file = file;
    file.write_all(bytes)
        .map_err(|e| format!("Cannot write stage {relative}: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("Cannot sync stage {relative}: {e}"))?;
    Ok(())
}

fn persist_stage(
    source_root: &Path,
    target: &Path,
    journal_path: &Path,
    journal: &mut AdoptionJournal,
    entry_index: usize,
    relative: &str,
) -> Result<()> {
    persist_stage_with_hook(
        source_root,
        target,
        journal_path,
        journal,
        entry_index,
        relative,
        || Ok(()),
    )
}

fn persist_stage_with_hook<F>(
    source_root: &Path,
    target: &Path,
    journal_path: &Path,
    journal: &mut AdoptionJournal,
    entry_index: usize,
    relative: &str,
    before_rename: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    let entry = journal
        .entries
        .get(entry_index)
        .ok_or("Adoption journal entry index is invalid")?
        .clone();
    let stage_path = safe_path(target, relative)?;
    let stage_state = file_state(target, relative)?;
    if stage_state.bytes != entry.after.bytes {
        return Err(format!(
            "Staged output changed before replacement: {relative}"
        ));
    }
    if file_state(target, &entry.relative)? != entry.before {
        return Err(format!("Target changed during apply: {}", entry.relative));
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&stage_path)
        .map_err(|e| format!("Cannot open stage {relative}: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("Cannot sync stage {relative}: {e}"))?;
    entry
        .after
        .permissions
        .as_ref()
        .ok_or("Journal postimage is missing permissions")?
        .apply_to(&stage_path)?;
    drop(file);
    prepare_destination_for_replacement(
        source_root,
        target,
        journal_path,
        journal,
        entry_index,
        &entry.before,
    )?;
    before_rename()?;
    let (stage_path, destination) = revalidate_stage_and_destination(
        source_root,
        target,
        journal_path,
        journal,
        entry_index,
        relative,
        &entry.after,
        &entry.before,
    )?;
    fs::rename(&stage_path, &destination)
        .map_err(|error| format!("Cannot atomically replace {}: {}", entry.relative, error))?;
    if file_state(target, &entry.relative)? != entry.after {
        return Err(format!(
            "Installed output did not match its journal: {}",
            entry.relative
        ));
    }
    set_entry_transition(
        source_root,
        target,
        journal_path,
        journal,
        entry_index,
        JournalTransition::Stable,
    )?;
    Ok(())
}

#[cfg(windows)]
fn make_stage_writable(path: &Path, relative: &str) -> Result<()> {
    let mut permissions = fs::metadata(path)
        .map_err(|e| format!("Cannot inspect adoption stage {relative}: {e}"))?
        .permissions();
    if permissions.readonly() {
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)
            .map_err(|e| format!("Cannot make adoption stage writable {relative}: {e}"))?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn make_stage_writable(_path: &Path, _relative: &str) -> Result<()> {
    Ok(())
}

fn writable_variant(state: &JournalFileState) -> JournalFileState {
    #[cfg(windows)]
    {
        let mut writable = state.clone();
        if let Some(permissions) = writable.permissions.as_mut() {
            permissions.readonly = false;
        }
        writable
    }
    #[cfg(not(windows))]
    {
        state.clone()
    }
}

fn set_entry_transition(
    source_root: &Path,
    target: &Path,
    journal_path: &Path,
    journal: &mut AdoptionJournal,
    entry_index: usize,
    transition: JournalTransition,
) -> Result<()> {
    let current = journal
        .entries
        .get(entry_index)
        .ok_or("Adoption journal entry index is invalid")?
        .transition;
    if current == transition {
        return Ok(());
    }
    let allowed = matches!(
        (current, transition),
        (JournalTransition::Stable, JournalTransition::Pending)
            | (
                JournalTransition::Pending,
                JournalTransition::AttributesCleared
            )
            | (JournalTransition::Pending, JournalTransition::Stable)
            | (
                JournalTransition::AttributesCleared,
                JournalTransition::Pending
            )
            | (
                JournalTransition::AttributesCleared,
                JournalTransition::Stable
            )
    );
    if !allowed {
        return Err("Invalid adoption journal destination transition".into());
    }

    validate_live_identity(source_root, target, &journal.identity)?;
    let mut updated = journal.clone();
    updated.entries[entry_index].transition = transition;
    seal_journal(&mut updated)?;
    write_journal_update(journal_path, journal, &updated)?;
    *journal = updated;
    Ok(())
}

fn prepare_destination_for_replacement(
    source_root: &Path,
    target: &Path,
    journal_path: &Path,
    journal: &mut AdoptionJournal,
    entry_index: usize,
    expected: &JournalFileState,
) -> Result<()> {
    let relative = journal
        .entries
        .get(entry_index)
        .ok_or("Adoption journal entry index is invalid")?
        .relative
        .clone();
    let writable = writable_variant(expected);
    let current = file_state(target, &relative)?;
    if current == writable && current != *expected {
        if journal.entries[entry_index].transition != JournalTransition::AttributesCleared {
            return Err(format!("Preserving concurrent edit at {relative}"));
        }
        return Ok(());
    }
    if current != *expected {
        return Err(format!("Target changed during replacement: {relative}"));
    }
    if writable == *expected {
        return Ok(());
    }

    // Persist intent before changing the read-only bit. If recovery sees a
    // writable variant while this entry is still pending, it must fail closed.
    set_entry_transition(
        source_root,
        target,
        journal_path,
        journal,
        entry_index,
        JournalTransition::Pending,
    )?;
    validate_live_identity(source_root, target, &journal.identity)?;
    if file_state(target, &relative)? != *expected {
        return Err(format!("Target changed during replacement: {relative}"));
    }

    #[cfg(windows)]
    {
        let path = safe_path(target, &relative)?;
        let mut permissions = fs::metadata(&path)
            .map_err(|e| format!("Cannot inspect replacement target {relative}: {e}"))?
            .permissions();
        if permissions.readonly() {
            permissions.set_readonly(false);
            fs::set_permissions(&path, permissions)
                .map_err(|e| format!("Cannot make replacement target writable {relative}: {e}"))?;
        }
    }
    if file_state(target, &relative)? != writable {
        return Err(format!(
            "Replacement target permissions did not become writable: {relative}"
        ));
    }

    // Only this durable marker authorizes recovery to accept writable variants.
    set_entry_transition(
        source_root,
        target,
        journal_path,
        journal,
        entry_index,
        JournalTransition::AttributesCleared,
    )?;
    Ok(())
}

fn cleanup_stage(target: &Path, relative: &str, expected: &JournalFileState) -> Result<()> {
    let path = safe_path(target, relative)?;
    if metadata(&path)?.is_none() {
        return Ok(());
    }
    if !stage_is_safe_prefix(target, relative, expected)? {
        return Err(format!(
            "Preserving unrecognized adoption stage: {relative}"
        ));
    }
    make_stage_writable(&path, relative)?;
    fs::remove_file(&path).map_err(|e| format!("Cannot remove adoption stage {relative}: {e}"))
}

fn destination_matches_expected(
    entry: &JournalEntry,
    current: &JournalFileState,
    expected: &JournalFileState,
) -> bool {
    current == expected
        || (current == &writable_variant(expected)
            && entry.transition == JournalTransition::AttributesCleared)
}

fn revalidate_journal_and_identity(
    source_root: &Path,
    target: &Path,
    journal_path: &Path,
    journal: &AdoptionJournal,
) -> Result<()> {
    if read_journal(journal_path)? != *journal {
        return Err("Adoption recovery journal changed before replacement".into());
    }
    validate_live_identity(source_root, target, &journal.identity)
}

fn revalidate_destination_for_removal(
    source_root: &Path,
    target: &Path,
    journal_path: &Path,
    journal: &AdoptionJournal,
    entry_index: usize,
    expected: &JournalFileState,
) -> Result<PathBuf> {
    let entry = journal
        .entries
        .get(entry_index)
        .ok_or("Adoption journal entry index is invalid")?;
    revalidate_journal_and_identity(source_root, target, journal_path, journal)?;
    let destination = safe_path(target, &entry.relative)?;
    let current = file_state(target, &entry.relative)?;
    if !destination_matches_expected(entry, &current, expected) {
        return Err(format!("Preserving concurrent edit at {}", entry.relative));
    }
    Ok(destination)
}

fn revalidate_stage_and_destination(
    source_root: &Path,
    target: &Path,
    journal_path: &Path,
    journal: &AdoptionJournal,
    entry_index: usize,
    stage_relative: &str,
    expected_stage: &JournalFileState,
    expected_destination: &JournalFileState,
) -> Result<(PathBuf, PathBuf)> {
    let entry = journal
        .entries
        .get(entry_index)
        .ok_or("Adoption journal entry index is invalid")?;
    revalidate_journal_and_identity(source_root, target, journal_path, journal)?;
    let stage_path = safe_path(target, stage_relative)?;
    if file_state(target, stage_relative)? != *expected_stage {
        return Err(format!(
            "Staged output changed before replacement: {stage_relative}"
        ));
    }
    let destination = safe_path(target, &entry.relative)?;
    let current = file_state(target, &entry.relative)?;
    if !destination_matches_expected(entry, &current, expected_destination) {
        return Err(format!(
            "Target changed during replacement: {}",
            entry.relative
        ));
    }
    Ok((stage_path, destination))
}

fn restore_entry(
    source_root: &Path,
    target: &Path,
    journal_path: &Path,
    journal: &mut AdoptionJournal,
    entry_index: usize,
) -> Result<bool> {
    let entry = journal
        .entries
        .get(entry_index)
        .ok_or("Adoption journal entry index is invalid")?
        .clone();
    validate_live_identity(source_root, target, &journal.identity)?;
    let current = file_state(target, &entry.relative)?;
    if current == entry.before {
        set_entry_transition(
            source_root,
            target,
            journal_path,
            journal,
            entry_index,
            JournalTransition::Stable,
        )?;
        return Ok(false);
    }
    let before_writable = writable_variant(&entry.before);
    if current == before_writable && current != entry.before {
        if journal.entries[entry_index].transition != JournalTransition::AttributesCleared {
            return Err(format!("Preserving concurrent edit at {}", entry.relative));
        }
        validate_live_identity(source_root, target, &journal.identity)?;
        if file_state(target, &entry.relative)? != before_writable {
            return Err(format!("Preserving concurrent edit at {}", entry.relative));
        }
        entry
            .before
            .permissions
            .as_ref()
            .ok_or("Journal preimage is missing permissions")?
            .apply_to(&safe_path(target, &entry.relative)?)?;
        if file_state(target, &entry.relative)? != entry.before {
            return Err(format!(
                "Restored preimage permissions did not match the journal: {}",
                entry.relative
            ));
        }
        set_entry_transition(
            source_root,
            target,
            journal_path,
            journal,
            entry_index,
            JournalTransition::Stable,
        )?;
        return Ok(false);
    }
    let after_writable = writable_variant(&entry.after);
    if current == after_writable
        && current != entry.after
        && journal.entries[entry_index].transition != JournalTransition::AttributesCleared
    {
        return Err(format!("Preserving concurrent edit at {}", entry.relative));
    }
    if current != entry.after && current != after_writable {
        return Err(format!("Preserving concurrent edit at {}", entry.relative));
    }
    validate_live_identity(source_root, target, &journal.identity)?;
    if entry.before.bytes.is_none() {
        let live = file_state(target, &entry.relative)?;
        if live != entry.after && live != after_writable {
            return Err(format!("Preserving concurrent edit at {}", entry.relative));
        }
        prepare_destination_for_replacement(
            source_root,
            target,
            journal_path,
            journal,
            entry_index,
            &entry.after,
        )?;
        let destination = revalidate_destination_for_removal(
            source_root,
            target,
            journal_path,
            journal,
            entry_index,
            &entry.after,
        )?;
        fs::remove_file(&destination)
            .map_err(|e| format!("Cannot remove installed {}: {e}", entry.relative))?;
        if file_state(target, &entry.relative)? != entry.before {
            return Err(format!(
                "Removed output did not match its preimage: {}",
                entry.relative
            ));
        }
        set_entry_transition(
            source_root,
            target,
            journal_path,
            journal,
            entry_index,
            JournalTransition::Stable,
        )?;
        return Ok(true);
    }

    let before_bytes = entry
        .before
        .bytes
        .as_deref()
        .ok_or("Missing journal preimage")?;
    let rollback_path = safe_path(target, &entry.rollback_stage)?;
    let mut stage_ready = false;
    if metadata(&rollback_path)?.is_some() {
        let staged = file_state(target, &entry.rollback_stage)?;
        if staged.bytes.as_deref() == Some(before_bytes) {
            stage_ready = true;
        } else if stage_is_safe_prefix(target, &entry.rollback_stage, &entry.before)? {
            validate_live_identity(source_root, target, &journal.identity)?;
            cleanup_stage(target, &entry.rollback_stage, &entry.before)?;
        } else {
            return Err(format!(
                "Preserving unrecognized rollback stage: {}",
                entry.rollback_stage
            ));
        }
    }
    if !stage_ready {
        create_stage(target, &entry.rollback_stage, before_bytes)?;
    }
    validate_live_identity(source_root, target, &journal.identity)?;
    let live = file_state(target, &entry.relative)?;
    if live != entry.after && live != after_writable {
        return Err(format!("Preserving concurrent edit at {}", entry.relative));
    }
    let stage_path = safe_path(target, &entry.rollback_stage)?;
    make_stage_writable(&stage_path, &entry.rollback_stage)?;
    let stage = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&stage_path)
        .map_err(|e| format!("Cannot open rollback stage {}: {e}", entry.rollback_stage))?;
    stage
        .sync_all()
        .map_err(|e| format!("Cannot sync rollback stage {}: {e}", entry.rollback_stage))?;
    drop(stage);
    entry
        .before
        .permissions
        .as_ref()
        .ok_or("Journal preimage is missing permissions")?
        .apply_to(&stage_path)?;
    prepare_destination_for_replacement(
        source_root,
        target,
        journal_path,
        journal,
        entry_index,
        &entry.after,
    )?;
    let (stage_path, destination) = revalidate_stage_and_destination(
        source_root,
        target,
        journal_path,
        journal,
        entry_index,
        &entry.rollback_stage,
        &entry.before,
        &entry.after,
    )?;
    fs::rename(&stage_path, &destination)
        .map_err(|e| format!("Cannot restore {}: {e}", entry.relative))?;
    if file_state(target, &entry.relative)? != entry.before {
        return Err(format!(
            "Restored output did not match its preimage: {}",
            entry.relative
        ));
    }
    set_entry_transition(
        source_root,
        target,
        journal_path,
        journal,
        entry_index,
        JournalTransition::Stable,
    )?;
    Ok(true)
}

fn remove_journal(
    source_root: &Path,
    target: &Path,
    path: &Path,
    identity: &JournalIdentity,
) -> Result<()> {
    validate_live_identity(source_root, target, identity)?;
    let (_, current_path) = journal_path(target)?;
    if current_path != path {
        return Err("Adoption recovery journal path changed".into());
    }
    match metadata(path)? {
        None => Ok(()),
        Some(value) if value.file_type().is_symlink() || !value.is_file() => {
            Err("Adoption recovery journal must be a regular file".into())
        }
        Some(_) => {
            fs::remove_file(path).map_err(|e| format!("Cannot remove recovery journal: {e}"))
        }
    }
}

fn recover_prepared(
    source_root: &Path,
    target: &Path,
    journal_path: &Path,
    journal: &mut AdoptionJournal,
) -> Result<()> {
    let mut problems = Vec::new();
    for entry_index in 0..journal.entries.len() {
        if let Err(error) = restore_entry(source_root, target, journal_path, journal, entry_index) {
            problems.push(error);
        }
    }
    if validate_live_identity(source_root, target, &journal.identity).is_ok() {
        for entry in &journal.entries {
            for (stage, expected) in [
                (&entry.stage, &entry.after),
                (&entry.rollback_stage, &entry.before),
            ] {
                if let Err(error) = validate_live_identity(source_root, target, &journal.identity)
                    .and_then(|()| cleanup_stage(target, stage, expected))
                {
                    problems.push(error);
                }
            }
        }
    } else {
        problems.push("Adoption target identity changed before stage cleanup".into());
    }
    if !problems.is_empty() {
        return Err(format!(
            "Adoption recovery is incomplete: {}",
            problems.join("; ")
        ));
    }
    validate_live_identity(source_root, target, &journal.identity)?;
    remove_journal(source_root, target, journal_path, &journal.identity)
}

fn cleanup_committed(
    source_root: &Path,
    target: &Path,
    journal_path: &Path,
    journal: &AdoptionJournal,
) -> Result<()> {
    let mut problems = Vec::new();
    for entry in &journal.entries {
        for (stage, expected) in [
            (&entry.stage, &entry.after),
            (&entry.rollback_stage, &entry.before),
        ] {
            if let Err(error) = validate_live_identity(source_root, target, &journal.identity)
                .and_then(|()| cleanup_stage(target, stage, expected))
            {
                problems.push(error);
            }
        }
    }
    if !problems.is_empty() {
        return Err(format!(
            "Committed adoption cleanup is incomplete: {}",
            problems.join("; ")
        ));
    }
    validate_live_identity(source_root, target, &journal.identity)?;
    remove_journal(source_root, target, journal_path, &journal.identity)
}

/// Restore the original managed outputs recorded by an interrupted adoption.
/// Conflicting user edits are preserved and keep the journal available to retry.
pub fn recover(root: &Path, target: &Path) -> Result<()> {
    let source_root = crate::security::checked_root(root)?;
    let target = validate_target(&source_root, target)?;
    let (git_dir, journal_path) = journal_path(&target)?;
    if metadata(&journal_path)?.is_none() {
        return Ok(());
    }
    let journal = read_journal(&journal_path)?;
    validate_journal(&source_root, &target, &git_dir, &journal)?;
    let mut journal = migrate_legacy_journal(&journal_path, &journal)?;
    match journal.state {
        JournalState::Prepared => {
            recover_prepared(&source_root, &target, &journal_path, &mut journal)
        }
        JournalState::Committed => {
            cleanup_committed(&source_root, &target, &journal_path, &journal)
        }
    }
}

pub fn ensure_no_pending_recovery(target: &Path) -> Result<()> {
    let target = absolute_target(target)?;
    let (_, journal_path) = journal_path(&target)?;
    if metadata(&journal_path)?.is_some() {
        return Err("Pending adoption recovery exists; run adopt TARGET --recover".into());
    }
    Ok(())
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
    apply_transaction(plan, None)
}

fn apply_transaction(
    plan: &AdoptionPlan,
    mut after_persist: Option<&mut AfterPersistHook>,
) -> Result<()> {
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
    let (_, pending_journal_path) = journal_path(&plan.target)?;
    if metadata(&pending_journal_path)?.is_some() {
        return Err("Pending adoption recovery exists; run `adopt TARGET --recover`".into());
    }
    let changed = plan
        .files
        .iter()
        .any(|(relative, content)| plan.baseline[relative].as_deref() != Some(content.as_bytes()));
    if !changed {
        return Ok(());
    }
    let mut journal = create_journal(&plan.target, &plan.source_root, plan)?;
    if validate_target(&plan.source_root, &plan.target)? != plan.checked_target
        || GitIdentity::read(&plan.target)? != plan.git_identity
    {
        return Err("Git working-tree identity changed after planning; create a fresh plan".into());
    }
    let (git_dir, verified_journal_path) = journal_path(&plan.target)?;
    validate_journal(&plan.source_root, &plan.target, &git_dir, &journal)?;
    if verified_journal_path != pending_journal_path {
        return Err("Adoption recovery journal path changed".into());
    }
    write_journal_exclusive(&pending_journal_path, &journal)?;

    let operation = (|| {
        for entry in &journal.entries {
            if entry.before == entry.after {
                continue;
            }
            validate_live_identity(&plan.source_root, &plan.target, &journal.identity)?;
            let output = entry
                .after
                .bytes
                .as_deref()
                .ok_or("Journal postimage is missing")?;
            create_stage(&plan.target, &entry.stage, output)?;
        }
        let mut written = 0usize;
        for entry_index in 0..journal.entries.len() {
            let entry = journal.entries[entry_index].clone();
            if entry.before == entry.after {
                continue;
            }
            validate_live_identity(&plan.source_root, &plan.target, &journal.identity)?;
            persist_stage(
                &plan.source_root,
                &plan.target,
                &pending_journal_path,
                &mut journal,
                entry_index,
                &entry.stage,
            )?;
            written += 1;
            if let Some(hook) = after_persist.as_deref_mut() {
                hook(written, &entry.relative)?;
            }
        }
        validate_live_identity(&plan.source_root, &plan.target, &journal.identity)?;
        for entry in &journal.entries {
            if file_state(&plan.target, &entry.relative)? != entry.after {
                return Err(format!(
                    "Installed output changed before commit: {}",
                    entry.relative
                ));
            }
        }
        let mut committed = journal.clone();
        committed.state = JournalState::Committed;
        seal_journal(&mut committed)?;
        write_journal_update(&pending_journal_path, &journal, &committed)?;
        journal = committed;
        validate_live_identity(&plan.source_root, &plan.target, &journal.identity)?;
        cleanup_committed(
            &plan.source_root,
            &plan.target,
            &pending_journal_path,
            &journal,
        )
    })();

    if let Err(error) = operation {
        if journal.state == JournalState::Committed {
            return Err(format!(
                "Adoption installed but committed cleanup is pending: {error}; run `adopt TARGET --recover`"
            ));
        }
        match recover_prepared(
            &plan.source_root,
            &plan.target,
            &pending_journal_path,
            &mut journal,
        ) {
            Ok(()) => Err(error),
            Err(recovery_error) => Err(format!(
                "Adoption failed: {error}; recovery remains pending: {recovery_error}; run `adopt TARGET --recover`"
            )),
        }
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_root() -> PathBuf {
        crate::security::checked_root(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn target_fixture() -> (tempfile::TempDir, PathBuf) {
        let scratch = tempfile::tempdir().unwrap();
        let target = scratch.path().join("target");
        fs::create_dir(&target).unwrap();
        isolated_git(&target, &["init", "-q"]);
        fs::create_dir(target.join(".ai-guidebook")).unwrap();
        let original_guide = b"# Original guide\n".to_vec();
        let source = source_repo(&source_root()).unwrap();
        let old_lock = serde_json::to_vec_pretty(&serde_json::json!({
            "source_repo": source,
            "guidebook_sha256": format!("{:x}", Sha256::digest(&original_guide)),
        }))
        .unwrap();
        fs::write(target.join(GUIDE), &original_guide).unwrap();
        fs::write(target.join(LOCK), old_lock).unwrap();
        fs::write(target.join("AGENTS.md"), b"Original agent instructions\n").unwrap();
        fs::write(target.join(".gitignore"), b"Original ignore rules\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(target.join(GUIDE), fs::Permissions::from_mode(0o640)).unwrap();
            fs::set_permissions(target.join(LOCK), fs::Permissions::from_mode(0o600)).unwrap();
            fs::set_permissions(target.join("AGENTS.md"), fs::Permissions::from_mode(0o644))
                .unwrap();
            fs::set_permissions(target.join(".gitignore"), fs::Permissions::from_mode(0o644))
                .unwrap();
        }
        (scratch, target.canonicalize().unwrap())
    }

    fn empty_target_fixture() -> (tempfile::TempDir, PathBuf) {
        let scratch = tempfile::tempdir().unwrap();
        let target = scratch.path().join("empty-target");
        fs::create_dir(&target).unwrap();
        isolated_git(&target, &["init", "-q"]);
        (scratch, target.canonicalize().unwrap())
    }

    fn fixture_plan(target: &Path) -> AdoptionPlan {
        let root = source_root();
        let target = target.canonicalize().unwrap();
        let guide = b"# Updated guide\n".to_vec();
        let source = source_repo(&root).unwrap();
        let lock = serde_json::to_vec_pretty(&serde_json::json!({
            "source_repo": source,
            "guidebook_sha256": format!("{:x}", Sha256::digest(&guide)),
        }))
        .unwrap();
        let files = BTreeMap::from([
            (GUIDE.to_owned(), String::from_utf8(guide).unwrap()),
            (LOCK.to_owned(), String::from_utf8(lock).unwrap()),
            (
                "AGENTS.md".to_owned(),
                "Updated agent instructions\n".to_owned(),
            ),
            (".gitignore".to_owned(), "Updated ignore rules\n".to_owned()),
        ]);
        let baseline = files
            .keys()
            .map(|relative| {
                (
                    relative.clone(),
                    file_state(&target, relative).unwrap().bytes,
                )
            })
            .collect();
        let identity = GitIdentity::read(&target).unwrap();
        AdoptionPlan {
            target: target.clone(),
            checked_target: target,
            checked_files: files.clone(),
            checked_adapters: Vec::new(),
            files,
            preserved_adapters: Vec::new(),
            source_root: root,
            baseline,
            git_identity: identity,
        }
    }

    fn target_snapshot(target: &Path) -> BTreeMap<String, JournalFileState> {
        [GUIDE, LOCK, "AGENTS.md", ".gitignore"]
            .into_iter()
            .map(|relative| (relative.to_owned(), file_state(target, relative).unwrap()))
            .collect()
    }

    #[cfg(windows)]
    fn set_readonly(target: &Path, relative: &str, readonly: bool) {
        let path = target.join(relative);
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(readonly);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[test]
    fn staging_paths_use_forward_slashes_on_every_platform() {
        let transaction = "a".repeat(64);
        let root = tempfile::tempdir().unwrap();

        for (rollback, suffix) in [(false, "install"), (true, "rollback")] {
            let stage =
                stage_relative(".ai-guidebook/GUIDEBOOK.md", &transaction, rollback).unwrap();
            assert_eq!(
                stage,
                format!(".ai-guidebook/.GUIDEBOOK.md.toe-tickling-{transaction}-{suffix}.stage")
            );
            safe_path(root.path(), &stage).unwrap();
        }
    }

    fn write_postimage(target: &Path, entry: &JournalEntry) {
        let path = target.join(&entry.relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        #[cfg(windows)]
        if path.exists() {
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            if permissions.readonly() {
                permissions.set_readonly(false);
                fs::set_permissions(&path, permissions).unwrap();
            }
        }
        fs::write(&path, entry.after.bytes.as_ref().unwrap()).unwrap();
        entry
            .after
            .permissions
            .as_ref()
            .unwrap()
            .apply_to(&path)
            .unwrap();
    }

    fn prepared_journal(target: &Path, plan: &AdoptionPlan) -> (PathBuf, AdoptionJournal) {
        let journal = create_journal(target, &plan.source_root, plan).unwrap();
        let (git_dir, journal_path) = journal_path(target).unwrap();
        validate_journal(&plan.source_root, target, &git_dir, &journal).unwrap();
        write_journal_exclusive(&journal_path, &journal).unwrap();
        let first = journal
            .entries
            .iter()
            .find(|entry| entry.before != entry.after)
            .unwrap();
        write_postimage(target, first);
        (journal_path, journal)
    }

    fn assert_no_stage_files(target: &Path) {
        fn visit(path: &Path) {
            for entry in fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    visit(&entry.path());
                } else {
                    assert!(
                        !entry
                            .file_name()
                            .to_string_lossy()
                            .contains(".toe-tickling-"),
                        "staging residue remains: {}",
                        entry.path().display()
                    );
                }
            }
        }
        visit(target);
    }

    #[test]
    fn recovery_rejects_wrong_target_without_writes() {
        let (_scratch, target) = target_fixture();
        let plan = fixture_plan(&target);
        let before = target_snapshot(&target);
        let mut journal = create_journal(&target, &plan.source_root, &plan).unwrap();
        journal.identity.target.push_str("-other");
        seal_journal(&mut journal).unwrap();
        let (_, journal_path) = journal_path(&target).unwrap();
        write_journal_exclusive(&journal_path, &journal).unwrap();

        let error = recover(&plan.source_root, &target).unwrap_err();

        assert!(error.contains("identity changed"));
        assert_eq!(target_snapshot(&target), before);
        assert!(journal_path.exists());
    }

    #[test]
    fn recovery_rejects_inconsistent_snapshot_lock_without_writes() {
        let (_scratch, target) = target_fixture();
        let plan = fixture_plan(&target);
        let before = target_snapshot(&target);
        let mut journal = create_journal(&target, &plan.source_root, &plan).unwrap();
        let guide = journal
            .entries
            .iter_mut()
            .find(|entry| entry.relative == GUIDE)
            .unwrap();
        guide.before.bytes = Some(b"tampered original guide\n".to_vec());
        seal_journal(&mut journal).unwrap();
        let (_, journal_path) = journal_path(&target).unwrap();
        write_journal_exclusive(&journal_path, &journal).unwrap();

        let error = recover(&plan.source_root, &target).unwrap_err();

        assert!(error.contains("Original guidebook and lock are inconsistent"));
        assert_eq!(target_snapshot(&target), before);
        assert!(journal_path.exists());
    }

    #[test]
    fn recovery_rejects_journal_missing_a_planned_output_without_writes() {
        let (_scratch, target) = target_fixture();
        let plan = fixture_plan(&target);
        let (journal_file, mut journal) = prepared_journal(&target, &plan);
        journal
            .entries
            .retain(|entry| entry.relative != "AGENTS.md");
        seal_journal(&mut journal).unwrap();
        let current = target_snapshot(&target);
        fs::write(&journal_file, serde_json::to_vec_pretty(&journal).unwrap()).unwrap();

        let error = recover(&plan.source_root, &target).unwrap_err();

        assert!(error.contains("planned output list does not match"));
        assert_eq!(target_snapshot(&target), current);
        assert!(journal_file.exists());
    }

    #[test]
    fn recovery_rejects_edited_journal_integrity_without_writes() {
        let (_scratch, target) = target_fixture();
        let plan = fixture_plan(&target);
        let (journal_file, mut journal) = prepared_journal(&target, &plan);
        let agents = journal
            .entries
            .iter_mut()
            .find(|entry| entry.relative == "AGENTS.md")
            .unwrap();
        agents.before.bytes = Some(b"unverified prior instructions\n".to_vec());
        let current = target_snapshot(&target);
        fs::write(&journal_file, serde_json::to_vec_pretty(&journal).unwrap()).unwrap();

        let error = recover(&plan.source_root, &target).unwrap_err();

        assert!(error.contains("journal integrity check failed"));
        assert_eq!(target_snapshot(&target), current);
        assert!(journal_file.exists());
    }

    #[test]
    fn recovery_rolls_back_after_process_exit() {
        if let Some(target) = std::env::var_os("TOE_TICKLING_ADOPT_TEST_EXIT_TARGET") {
            let target = PathBuf::from(target);
            let plan = fixture_plan(&target);
            let mut exit_after_first = |written: usize, _relative: &str| {
                if written == 1 {
                    std::process::exit(91);
                }
                Ok(())
            };
            let _ = apply_transaction(&plan, Some(&mut exit_after_first));
            panic!("child did not exit after its first replacement");
        }

        let (_scratch, target) = target_fixture();
        let plan = fixture_plan(&target);
        let before = target_snapshot(&target);
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "adopt::tests::recovery_rolls_back_after_process_exit",
                "--nocapture",
            ])
            .env("TOE_TICKLING_ADOPT_TEST_EXIT_TARGET", &target)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(91),
            "child did not stop after one persisted output:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_ne!(target_snapshot(&target), before);

        recover(&plan.source_root, &target).unwrap();

        assert_eq!(target_snapshot(&target), before);
        assert!(!journal_path(&target).unwrap().1.exists());
        assert_no_stage_files(&target);
    }

    #[test]
    fn apply_rolls_back_after_later_write_failure() {
        let (_scratch, target) = target_fixture();
        let plan = fixture_plan(&target);
        let before = target_snapshot(&target);
        let mut fail_after_first = |written: usize, _relative: &str| {
            if written == 1 {
                Err("injected later write failure".into())
            } else {
                Ok(())
            }
        };

        let error = apply_transaction(&plan, Some(&mut fail_after_first)).unwrap_err();

        assert!(error.contains("injected later write failure"));
        assert_eq!(target_snapshot(&target), before);
        assert!(!journal_path(&target).unwrap().1.exists());
        assert_no_stage_files(&target);
    }

    #[test]
    fn apply_commits_all_outputs_and_cleans_recovery_metadata() {
        let (_scratch, target) = target_fixture();
        let plan = fixture_plan(&target);
        let expected = create_journal(&target, &plan.source_root, &plan).unwrap();

        apply(&plan).unwrap();

        for entry in &expected.entries {
            assert_eq!(file_state(&target, &entry.relative).unwrap(), entry.after);
        }
        assert!(!journal_path(&target).unwrap().1.exists());
        assert_no_stage_files(&target);
    }

    #[cfg(windows)]
    #[test]
    fn recovery_preserves_windows_readonly_permissions() {
        let (_scratch, target) = target_fixture();
        for relative in [GUIDE, LOCK, "AGENTS.md", ".gitignore"] {
            set_readonly(&target, relative, true);
        }
        let plan = fixture_plan(&target);
        let expected = create_journal(&target, &plan.source_root, &plan).unwrap();

        apply(&plan).unwrap();

        for entry in &expected.entries {
            assert_eq!(file_state(&target, &entry.relative).unwrap(), entry.after);
        }

        let (_scratch, target) = target_fixture();
        for relative in [GUIDE, LOCK, "AGENTS.md", ".gitignore"] {
            set_readonly(&target, relative, true);
        }
        let plan = fixture_plan(&target);
        let before = target_snapshot(&target);
        let (journal_file, journal) = prepared_journal(&target, &plan);

        recover(&plan.source_root, &target).unwrap();

        assert_eq!(target_snapshot(&target), before);
        assert!(!journal_file.exists());
        assert_no_stage_files(&target);
        assert_eq!(journal.entries.len(), 4);
    }

    #[cfg(windows)]
    #[test]
    fn recovery_rejects_unjournaled_windows_readonly_clear() {
        let (_scratch, target) = target_fixture();
        set_readonly(&target, GUIDE, true);
        let plan = fixture_plan(&target);
        let before = file_state(&target, GUIDE).unwrap();
        let journal = create_journal(&target, &plan.source_root, &plan).unwrap();
        let (git_dir, journal_file) = journal_path(&target).unwrap();
        validate_journal(&plan.source_root, &target, &git_dir, &journal).unwrap();
        write_journal_exclusive(&journal_file, &journal).unwrap();

        set_readonly(&target, GUIDE, false);
        let user_state = file_state(&target, GUIDE).unwrap();
        assert_eq!(user_state.bytes, before.bytes);
        assert_ne!(user_state, before);

        let error = recover(&plan.source_root, &target).unwrap_err();

        assert!(error.contains("Preserving concurrent edit"), "{error}");
        assert_eq!(file_state(&target, GUIDE).unwrap(), user_state);
        assert!(journal_file.exists());
    }

    #[cfg(windows)]
    #[test]
    fn apply_preserves_destination_edit_between_marker_and_promotion() {
        let (_scratch, target) = target_fixture();
        set_readonly(&target, GUIDE, true);
        let plan = fixture_plan(&target);
        let mut journal = create_journal(&target, &plan.source_root, &plan).unwrap();
        let entry_index = journal
            .entries
            .iter()
            .position(|entry| entry.relative == GUIDE)
            .unwrap();
        let entry = journal.entries[entry_index].clone();
        let (git_dir, journal_file) = journal_path(&target).unwrap();
        validate_journal(&plan.source_root, &target, &git_dir, &journal).unwrap();
        write_journal_exclusive(&journal_file, &journal).unwrap();
        create_stage(&target, &entry.stage, entry.after.bytes.as_deref().unwrap()).unwrap();
        let user_edit = b"user edit after journal authorization\n";

        let error = persist_stage_with_hook(
            &plan.source_root,
            &target,
            &journal_file,
            &mut journal,
            entry_index,
            &entry.stage,
            || {
                let persisted = read_journal(&journal_file)?;
                assert_eq!(
                    persisted.entries[entry_index].transition,
                    JournalTransition::AttributesCleared
                );
                fs::write(target.join(GUIDE), user_edit).map_err(|error| error.to_string())
            },
        )
        .unwrap_err();

        assert!(
            error.contains("Target changed during replacement"),
            "{error}"
        );
        assert_eq!(
            file_state(&target, GUIDE).unwrap().bytes,
            Some(user_edit.to_vec())
        );
        assert!(journal_file.exists());
        assert!(safe_path(&target, &entry.stage).unwrap().exists());
        assert_eq!(
            read_journal(&journal_file).unwrap().entries[entry_index].transition,
            JournalTransition::AttributesCleared
        );
        make_stage_writable(&safe_path(&target, &entry.stage).unwrap(), &entry.stage).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn apply_preserves_destination_when_stage_changes_after_marker() {
        let (_scratch, target) = target_fixture();
        set_readonly(&target, GUIDE, true);
        let plan = fixture_plan(&target);
        let mut journal = create_journal(&target, &plan.source_root, &plan).unwrap();
        let entry_index = journal
            .entries
            .iter()
            .position(|entry| entry.relative == GUIDE)
            .unwrap();
        let entry = journal.entries[entry_index].clone();
        let (git_dir, journal_file) = journal_path(&target).unwrap();
        validate_journal(&plan.source_root, &target, &git_dir, &journal).unwrap();
        write_journal_exclusive(&journal_file, &journal).unwrap();
        create_stage(&target, &entry.stage, entry.after.bytes.as_deref().unwrap()).unwrap();
        let original = file_state(&target, GUIDE).unwrap();

        let error = persist_stage_with_hook(
            &plan.source_root,
            &target,
            &journal_file,
            &mut journal,
            entry_index,
            &entry.stage,
            || {
                let persisted = read_journal(&journal_file)?;
                assert_eq!(
                    persisted.entries[entry_index].transition,
                    JournalTransition::AttributesCleared
                );
                let stage_path = safe_path(&target, &entry.stage)?;
                make_stage_writable(&stage_path, &entry.stage)?;
                fs::write(stage_path, b"unrecognized stage content\n")
                    .map_err(|error| error.to_string())
            },
        )
        .unwrap_err();

        assert!(
            error.contains("Staged output changed before replacement"),
            "{error}"
        );
        assert_eq!(
            file_state(&target, GUIDE).unwrap(),
            writable_variant(&original)
        );
        assert!(journal_file.exists());
        assert_eq!(
            read_journal(&journal_file).unwrap().entries[entry_index].transition,
            JournalTransition::AttributesCleared
        );
    }

    #[cfg(windows)]
    #[test]
    fn recovery_accepts_recorded_windows_readonly_clear() {
        let (_scratch, target) = target_fixture();
        set_readonly(&target, GUIDE, true);
        let plan = fixture_plan(&target);
        let before = target_snapshot(&target);
        let mut journal = create_journal(&target, &plan.source_root, &plan).unwrap();
        let entry_index = journal
            .entries
            .iter()
            .position(|entry| entry.relative == GUIDE)
            .unwrap();
        let expected_before = journal.entries[entry_index].before.clone();
        let (git_dir, journal_file) = journal_path(&target).unwrap();
        validate_journal(&plan.source_root, &target, &git_dir, &journal).unwrap();
        write_journal_exclusive(&journal_file, &journal).unwrap();
        prepare_destination_for_replacement(
            &plan.source_root,
            &target,
            &journal_file,
            &mut journal,
            entry_index,
            &expected_before,
        )
        .unwrap();

        recover(&plan.source_root, &target).unwrap();

        assert_eq!(target_snapshot(&target), before);
        assert!(!journal_file.exists());
    }

    #[cfg(windows)]
    #[test]
    fn recovery_refuses_pending_windows_readonly_clear() {
        let (_scratch, target) = target_fixture();
        set_readonly(&target, GUIDE, true);
        let plan = fixture_plan(&target);
        let mut journal = create_journal(&target, &plan.source_root, &plan).unwrap();
        let entry_index = journal
            .entries
            .iter()
            .position(|entry| entry.relative == GUIDE)
            .unwrap();
        let (git_dir, journal_file) = journal_path(&target).unwrap();
        validate_journal(&plan.source_root, &target, &git_dir, &journal).unwrap();
        write_journal_exclusive(&journal_file, &journal).unwrap();
        set_entry_transition(
            &plan.source_root,
            &target,
            &journal_file,
            &mut journal,
            entry_index,
            JournalTransition::Pending,
        )
        .unwrap();
        set_readonly(&target, GUIDE, false);
        let user_state = file_state(&target, GUIDE).unwrap();

        let error = recover(&plan.source_root, &target).unwrap_err();

        assert!(error.contains("Preserving concurrent edit"), "{error}");
        assert_eq!(file_state(&target, GUIDE).unwrap(), user_state);
        let stored = read_journal(&journal_file).unwrap();
        assert_eq!(
            stored.entries[entry_index].transition,
            JournalTransition::Pending
        );
    }

    #[cfg(windows)]
    #[test]
    fn recovery_migrates_v2_journal_before_permission_changes() {
        let (_scratch, target) = target_fixture();
        set_readonly(&target, GUIDE, true);
        let plan = fixture_plan(&target);
        let journal = create_journal(&target, &plan.source_root, &plan).unwrap();
        let (git_dir, journal_file) = journal_path(&target).unwrap();
        validate_journal(&plan.source_root, &target, &git_dir, &journal).unwrap();
        write_journal_exclusive(&journal_file, &journal).unwrap();

        let mut legacy_value = serde_json::to_value(&journal).unwrap();
        let object = legacy_value.as_object_mut().unwrap();
        object.insert(
            "version".into(),
            serde_json::json!(LEGACY_JOURNAL_V2_VERSION),
        );
        object.insert("integrity_sha256".into(), serde_json::json!(""));
        for entry in object.get_mut("entries").unwrap().as_array_mut().unwrap() {
            entry.as_object_mut().unwrap().remove("transition");
        }
        let mut legacy: AdoptionJournal = serde_json::from_value(legacy_value).unwrap();
        seal_journal(&mut legacy).unwrap();
        let mut legacy_value = serde_json::to_value(&legacy).unwrap();
        for entry in legacy_value
            .get_mut("entries")
            .unwrap()
            .as_array_mut()
            .unwrap()
        {
            entry.as_object_mut().unwrap().remove("transition");
        }
        fs::write(
            &journal_file,
            serde_json::to_vec_pretty(&legacy_value).unwrap(),
        )
        .unwrap();

        set_readonly(&target, GUIDE, false);
        let user_state = file_state(&target, GUIDE).unwrap();
        let entry_index = journal
            .entries
            .iter()
            .position(|entry| entry.relative == GUIDE)
            .unwrap();

        let error = recover(&plan.source_root, &target).unwrap_err();

        assert!(error.contains("Preserving concurrent edit"), "{error}");
        assert_eq!(file_state(&target, GUIDE).unwrap(), user_state);
        let migrated = read_journal(&journal_file).unwrap();
        assert_eq!(migrated.version, JOURNAL_VERSION);
        assert_eq!(
            migrated.entries[entry_index].transition,
            JournalTransition::Stable
        );
    }

    #[cfg(windows)]
    #[test]
    fn recovery_removes_readonly_stage_left_before_promotion() {
        let (_scratch, target) = target_fixture();
        set_readonly(&target, GUIDE, true);
        let plan = fixture_plan(&target);
        let journal = create_journal(&target, &plan.source_root, &plan).unwrap();
        let entry = journal
            .entries
            .iter()
            .find(|entry| entry.relative == GUIDE)
            .unwrap();
        let (git_dir, journal_file) = journal_path(&target).unwrap();
        validate_journal(&plan.source_root, &target, &git_dir, &journal).unwrap();
        write_journal_exclusive(&journal_file, &journal).unwrap();

        let bytes = entry.after.bytes.as_deref().unwrap();
        create_stage(&target, &entry.stage, &bytes[..1]).unwrap();
        entry
            .after
            .permissions
            .as_ref()
            .unwrap()
            .apply_to(&safe_path(&target, &entry.stage).unwrap())
            .unwrap();

        recover(&plan.source_root, &target).unwrap();

        assert!(!journal_file.exists());
        assert!(!safe_path(&target, &entry.stage).unwrap().exists());
        assert_eq!(file_state(&target, GUIDE).unwrap(), entry.before);
    }

    #[cfg(windows)]
    #[test]
    fn recovery_reopens_a_complete_readonly_rollback_stage() {
        let (_scratch, target) = target_fixture();
        set_readonly(&target, GUIDE, true);
        let plan = fixture_plan(&target);
        let before = target_snapshot(&target);
        let (journal_file, mut journal) = prepared_journal(&target, &plan);
        let entry_index = journal
            .entries
            .iter()
            .position(|entry| entry.relative == GUIDE)
            .unwrap();
        let entry = journal.entries[entry_index].clone();
        create_stage(
            &target,
            &entry.rollback_stage,
            entry.before.bytes.as_deref().unwrap(),
        )
        .unwrap();
        entry
            .before
            .permissions
            .as_ref()
            .unwrap()
            .apply_to(&safe_path(&target, &entry.rollback_stage).unwrap())
            .unwrap();
        prepare_destination_for_replacement(
            &plan.source_root,
            &target,
            &journal_file,
            &mut journal,
            entry_index,
            &entry.after,
        )
        .unwrap();

        recover(&plan.source_root, &target).unwrap();

        assert_eq!(target_snapshot(&target), before);
        assert!(!journal_file.exists());
        assert_no_stage_files(&target);
    }

    #[cfg(windows)]
    #[test]
    fn recovery_restores_readonly_preimage_after_interrupted_promotion() {
        let (_scratch, target) = target_fixture();
        set_readonly(&target, GUIDE, true);
        let plan = fixture_plan(&target);
        let before = target_snapshot(&target);
        let mut journal = create_journal(&target, &plan.source_root, &plan).unwrap();
        let entry_index = journal
            .entries
            .iter()
            .position(|entry| entry.relative == GUIDE)
            .unwrap();
        let entry = journal.entries[entry_index].clone();
        let (git_dir, journal_file) = journal_path(&target).unwrap();
        validate_journal(&plan.source_root, &target, &git_dir, &journal).unwrap();
        write_journal_exclusive(&journal_file, &journal).unwrap();

        create_stage(&target, &entry.stage, entry.after.bytes.as_deref().unwrap()).unwrap();
        entry
            .after
            .permissions
            .as_ref()
            .unwrap()
            .apply_to(&safe_path(&target, &entry.stage).unwrap())
            .unwrap();
        prepare_destination_for_replacement(
            &plan.source_root,
            &target,
            &journal_file,
            &mut journal,
            entry_index,
            &entry.before,
        )
        .unwrap();
        fs::rename(
            safe_path(&target, &entry.stage).unwrap(),
            safe_path(&target, GUIDE).unwrap(),
        )
        .unwrap();

        recover(&plan.source_root, &target).unwrap();

        assert_eq!(target_snapshot(&target), before);
        assert!(!journal_file.exists());
        assert_no_stage_files(&target);
    }

    #[test]
    fn apply_rejects_a_pending_recovery_journal() {
        let (_scratch, target) = target_fixture();
        let stale_plan = fixture_plan(&target);
        let _ = prepared_journal(&target, &stale_plan);
        let current_plan = fixture_plan(&target);

        let error = apply(&current_plan).unwrap_err();

        assert!(error.contains("Pending adoption recovery"));
        assert!(journal_path(&target).unwrap().1.exists());
    }

    #[test]
    fn recovery_preserves_concurrent_user_edit() {
        let (_scratch, target) = target_fixture();
        let plan = fixture_plan(&target);
        let (journal_file, journal) = prepared_journal(&target, &plan);
        let lock = journal
            .entries
            .iter()
            .find(|entry| entry.relative == LOCK)
            .unwrap();
        fs::write(target.join(LOCK), b"concurrent user edit\n").unwrap();

        let error = recover(&plan.source_root, &target).unwrap_err();

        assert!(error.contains("Preserving concurrent edit"));
        assert_eq!(
            file_state(&target, GUIDE).unwrap(),
            journal
                .entries
                .iter()
                .find(|entry| entry.relative == GUIDE)
                .unwrap()
                .before
        );
        assert_eq!(
            fs::read(target.join(LOCK)).unwrap(),
            b"concurrent user edit\n"
        );
        assert!(journal_file.exists());
        assert_eq!(
            file_state(&target, LOCK).unwrap(),
            JournalFileState {
                bytes: Some(b"concurrent user edit\n".to_vec()),
                permissions: lock.before.permissions.clone(),
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn recovery_rejects_symlinked_ancestor() {
        use std::os::unix::fs::symlink;

        let (_scratch, target) = target_fixture();
        let plan = fixture_plan(&target);
        let (journal_file, _) = prepared_journal(&target, &plan);
        let outside = target.parent().unwrap().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"outside bytes").unwrap();
        fs::remove_dir_all(target.join(".ai-guidebook")).unwrap();
        symlink(&outside, target.join(".ai-guidebook")).unwrap();

        let error = recover(&plan.source_root, &target).unwrap_err();

        assert!(error.contains("symlink"));
        assert_eq!(
            fs::read(outside.join("sentinel")).unwrap(),
            b"outside bytes"
        );
        assert!(journal_file.exists());
    }

    #[test]
    fn recovery_cleans_staged_files() {
        let (_scratch, target) = target_fixture();
        let plan = fixture_plan(&target);
        let (journal_file, journal) = prepared_journal(&target, &plan);
        for entry in &journal.entries {
            if entry.before != entry.after {
                create_stage(&target, &entry.stage, entry.after.bytes.as_deref().unwrap()).unwrap();
            }
        }
        recover(&plan.source_root, &target).unwrap();
        assert!(!journal_file.exists());
        assert_no_stage_files(&target);
    }

    #[test]
    fn recovery_removes_newly_installed_outputs() {
        let (_scratch, target) = empty_target_fixture();
        let plan = fixture_plan(&target);
        let journal = create_journal(&target, &plan.source_root, &plan).unwrap();
        let (_, journal_file) = journal_path(&target).unwrap();
        write_journal_exclusive(&journal_file, &journal).unwrap();
        for entry in &journal.entries {
            write_postimage(&target, entry);
        }

        recover(&plan.source_root, &target).unwrap();

        for relative in [GUIDE, LOCK, "AGENTS.md", ".gitignore"] {
            assert!(file_state(&target, relative).unwrap().bytes.is_none());
        }
        assert!(!journal_file.exists());
        assert_no_stage_files(&target);
    }

    #[test]
    fn recovery_cleans_committed_journal_without_rollback() {
        let (_scratch, target) = target_fixture();
        let plan = fixture_plan(&target);
        let mut journal = create_journal(&target, &plan.source_root, &plan).unwrap();
        for entry in &journal.entries {
            if entry.before != entry.after {
                write_postimage(&target, entry);
            }
        }
        let first = journal
            .entries
            .iter()
            .find(|entry| entry.before != entry.after)
            .unwrap();
        let stage = first.stage.clone();
        let expected_after = first.after.clone();
        let stage_bytes = first.after.bytes.as_deref().unwrap().to_vec();
        create_stage(&target, &stage, &stage_bytes).unwrap();
        journal.state = JournalState::Committed;
        seal_journal(&mut journal).unwrap();
        let (_, journal_file) = journal_path(&target).unwrap();
        write_journal_exclusive(&journal_file, &journal).unwrap();

        recover(&plan.source_root, &target).unwrap();

        assert_eq!(file_state(&target, GUIDE).unwrap(), expected_after);
        assert!(!journal_file.exists());
        assert_no_stage_files(&target);
    }

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

    #[test]
    fn recovery_restores_legacy_v1_journal() {
        let (_scratch, target) = target_fixture();
        let plan = fixture_plan(&target);
        let before = target_snapshot(&target);
        let (journal_file, journal) = prepared_journal(&target, &plan);

        let mut legacy = serde_json::to_value(&journal).unwrap();
        let object = legacy.as_object_mut().unwrap();
        object.insert("version".into(), serde_json::json!(1));
        object.remove("planned_paths");
        object.remove("integrity_sha256");
        for entry in object.get_mut("entries").unwrap().as_array_mut().unwrap() {
            entry.as_object_mut().unwrap().remove("transition");
        }
        fs::write(&journal_file, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
        fs::write(target.join(LOCK), b"intervening user edit\n").unwrap();

        let error = recover(&plan.source_root, &target).unwrap_err();

        assert!(error.contains("Preserving concurrent edit"), "{error}");
        let migrated = read_journal(&journal_file).unwrap();
        assert_eq!(migrated.version, JOURNAL_VERSION);
        assert_eq!(
            migrated.planned_paths,
            migrated
                .entries
                .iter()
                .map(|entry| entry.relative.clone())
                .collect::<Vec<_>>()
        );
        let (git_dir, _) = journal_path(&target).unwrap();
        validate_journal(&plan.source_root, &target, &git_dir, &migrated).unwrap();
        let lock = migrated
            .entries
            .iter()
            .find(|entry| entry.relative == LOCK)
            .unwrap();
        let lock_path = safe_path(&target, LOCK).unwrap();
        fs::write(&lock_path, lock.before.bytes.as_deref().unwrap()).unwrap();
        lock.before
            .permissions
            .as_ref()
            .unwrap()
            .apply_to(&lock_path)
            .unwrap();

        recover(&plan.source_root, &target).unwrap();

        assert_eq!(target_snapshot(&target), before);
        assert!(!journal_file.exists());
        assert_no_stage_files(&target);
    }
}
