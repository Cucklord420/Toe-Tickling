//! Mechanical validation only: these checks do not establish truthful model behavior.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
};
use time::{Date, Month, OffsetDateTime, format_description::well_known::Rfc3339};

pub mod adopt;
pub mod guide;
pub mod security;

pub type Result<T> = std::result::Result<T, String>;

const GIT_CONTEXT_REDIRECTS: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_CEILING_DIRECTORIES",
    "GIT_DISCOVERY_ACROSS_FILESYSTEM",
];

/// Run Git against the selected checkout without inherited repository redirects.
/// Documented `GIT_CONFIG_*` settings remain available; repository-root callers
/// compare Git's resolved worktree with the selected path and fail closed on a
/// mismatch.
pub(crate) fn git_command(root: &Path) -> std::process::Command {
    let mut command = std::process::Command::new("git");
    command.current_dir(root);
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if GIT_CONTEXT_REDIRECTS
            .iter()
            .any(|redirect| name.eq_ignore_ascii_case(redirect))
        {
            command.env_remove(key);
        }
    }
    command
}

pub const READING_FILES: &[&str] = &[
    "AGENTS.md",
    "README.md",
    "RULEBOOK.md",
    "docs/SELF_CHECKS.md",
    "docs/EVOLUTION.md",
    "docs/ADOPTION.md",
    "docs/PROVENANCE.md",
    "docs/SCENARIOS.md",
    "CONTRIBUTING.md",
    "policy/rules.json",
    "llms.txt",
    "GUIDEBOOK.md",
    "policy.json",
    "guidebook-lock.json",
    "docs/INSTRUCTION_AUDIT.md",
    "CLAUDE.md",
    "GEMINI.md",
    ".github/copilot-instructions.md",
    ".cursor/rules/justin-guidebook.mdc",
    "prompts/USE_WITH_ANY_MODEL.md",
    "prompts/STANDING_MEMORY.md",
];
const SOURCES: &[&str] = &[
    "public-existing",
    "private-reviewed",
    "synthesis",
    "user-current",
];
const SCOPES: &[&str] = &[
    "global",
    "automation",
    "destructive",
    "recovery",
    "software",
    "purchase",
    "finance",
    "ev",
    "research",
];
// Stable IDs remain required when superseded; generated views retain policy history.
const STABLE_VERIFICATION_RULE_FAMILIES: &[(&str, usize)] = &[
    ("AUTH", 6),
    ("EVID", 6),
    ("EXEC", 8),
    ("SAFE", 6),
    ("ENG", 7),
    ("DOM", 5),
    ("AUD", 8),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub title: String,
    pub category: String,
    pub scope: String,
    pub status: String,
    pub requirement: String,
    pub verification: String,
    pub source: String,
    pub reviewed_on: String,
    pub review_due: String,
    pub supersedes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: u32,
    pub rules: Vec<Rule>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub documents: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reading {
    pub path: String,
    pub sha256: String,
    pub note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub id: String,
    pub status: String,
    pub reason: String,
    pub evidence: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Audit {
    pub schema_version: u32,
    pub task: String,
    pub outcome: String,
    pub recorded_at: String,
    pub repository_revision: String,
    pub reading: Vec<Reading>,
    pub evidence: BTreeMap<String, String>,
    pub checks: Vec<Check>,
    pub acceptance: Vec<Check>,
}
fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
pub fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}
pub fn write_json<T: Serialize>(path: &Path, data: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(data).map_err(|e| e.to_string())? + "\n";
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
pub fn sha256(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn date(value: &str) -> Result<Date> {
    let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
        .map_err(|e| e.to_string())?;
    Date::parse(value, &format).map_err(|e| format!("Invalid date {value}: {e}"))
}
fn useful(value: &str) -> bool {
    let value = value.trim();
    value.len() >= 12
        && !["TODO", "PLACEHOLDER", "UNFILLED"]
            .iter()
            .any(|s| value.to_uppercase().contains(s))
}

/// Return the policy calendar date for the owner's configured America/New_York timezone.
/// RFC3339 audit timestamps remain UTC; only date-only policy deadlines use this calendar.
pub fn policy_date_at(now: OffsetDateTime) -> Result<Date> {
    use chrono::Datelike;

    let utc =
        chrono::DateTime::<chrono::Utc>::from_timestamp(now.unix_timestamp(), now.nanosecond())
            .ok_or("Current timestamp is outside the timezone conversion range")?;
    let local = utc.with_timezone(&chrono_tz::America::New_York);
    let month_number = u8::try_from(local.month()).map_err(|e| e.to_string())?;
    let month = Month::try_from(month_number).map_err(|e| e.to_string())?;
    let day = u8::try_from(local.day()).map_err(|e| e.to_string())?;
    Date::from_calendar_date(local.year(), month, day).map_err(|e| e.to_string())
}

pub fn load_policy(root: &Path) -> Result<Policy> {
    read_json(&root.join("policy/rules.json"))
}
pub fn validate_policy(policy: &Policy, today: Date) -> Result<()> {
    require(policy.schema_version == 1, "Unsupported policy schema")?;
    require(!policy.rules.is_empty(), "Empty policy")?;
    let mut ids = BTreeSet::new();
    for r in &policy.rules {
        require(ids.insert(r.id.clone()), format!("Duplicate rule {}", r.id))?;
        let valid_id = r.id.split_once('-').is_some_and(|(prefix, number)| {
            !prefix.is_empty()
                && prefix.bytes().all(|c| c.is_ascii_uppercase())
                && number.len() == 2
                && number.bytes().all(|c| c.is_ascii_digit())
        });
        require(valid_id, format!("Invalid rule ID {}", r.id))?;
        require(
            !r.title.trim().is_empty()
                && !r.category.trim().is_empty()
                && useful(&r.requirement)
                && useful(&r.verification),
            format!("Incomplete rule {}", r.id),
        )?;
        require(
            SCOPES.contains(&r.scope.as_str()),
            format!("Invalid scope {}", r.id),
        )?;
        require(
            SOURCES.contains(&r.source.as_str()),
            format!("Invalid source {}", r.id),
        )?;
        require(
            ["active", "superseded"].contains(&r.status.as_str()),
            format!("Invalid status {}", r.id),
        )?;
        let reviewed = date(&r.reviewed_on)?;
        let due = date(&r.review_due)?;
        require(
            reviewed <= today && reviewed <= due,
            format!("Invalid review dates {}", r.id),
        )?;
        require(
            r.status != "active" || due >= today,
            format!(
                "Overdue review {} ({}); investigate, do not blindly extend",
                r.id, due
            ),
        )?;
    }
    for (family, count) in STABLE_VERIFICATION_RULE_FAMILIES {
        for number in 1..=*count {
            let id = format!("{family}-{number:02}");
            require(
                ids.contains(&id),
                format!("Missing existing verification rule {id}; preserve stable IDs and history"),
            )?;
        }
    }
    let mut replacements = BTreeSet::new();
    for r in &policy.rules {
        for old in &r.supersedes {
            require(
                old != &r.id && ids.contains(old),
                format!("Invalid supersession {} -> {}", r.id, old),
            )?;
            require(
                replacements.insert(old),
                format!("Multiple replacements for {old}"),
            )?;
            require(
                policy
                    .rules
                    .iter()
                    .any(|x| &x.id == old && x.status == "superseded"),
                format!("Replaced rule {old} still active"),
            )?;
        }
    }
    for r in policy.rules.iter().filter(|r| r.status == "superseded") {
        require(
            replacements.contains(&r.id),
            format!("Superseded rule {} lacks replacement", r.id),
        )?;
        // Follow replacement edges to reject cycles even when all members are superseded.
        let mut seen = BTreeSet::new();
        let mut current = &r.id;
        while let Some(next) = policy.rules.iter().find(|x| x.supersedes.contains(current)) {
            require(
                seen.insert(current),
                format!("Supersession cycle at {current}"),
            )?;
            current = &next.id;
        }
    }
    Ok(())
}
pub fn render(policy: &Policy) -> String {
    let mut out = String::from(
        "# The living rule book\n\nBinding operating requirements within the instruction hierarchy in AUTH-01. MUST means required when applicable. A scoped rule applies only to its named activity. Mechanical validation cannot prove honest behavior.\n\nThis file is generated from [policy/rules.json](policy/rules.json). Edit that source and run `cargo run --locked -- render`; do not maintain a competing copy. Read every section, then complete the [reading checkpoint](docs/SELF_CHECKS.md).\n\n",
    );
    let mut categories = Vec::new();
    for r in &policy.rules {
        if !categories.contains(&r.category) {
            categories.push(r.category.clone());
        }
    }
    for category in categories {
        out.push_str(&format!("## {category}\n\n"));
        for r in policy.rules.iter().filter(|r| r.category == category) {
            out.push_str(&format!(
                "### {} — {}\n\n**Scope:** {}. **Status:** {}.\n\n{}\n\n**Self-check:** {}\n\n",
                r.id, r.title, r.scope, r.status, r.requirement, r.verification
            ));
        }
    }
    out
}
pub fn current_manifest(root: &Path) -> Result<Manifest> {
    let mut documents = BTreeMap::new();
    for path in READING_FILES.iter().chain(guide::SOURCES.iter()) {
        documents.insert((*path).to_owned(), sha256(&root.join(path))?);
    }
    Ok(Manifest {
        schema_version: 1,
        documents,
    })
}
pub fn validate_links(root: &Path) -> Result<()> {
    security::check(root)
}
pub fn check_repo(root: &Path, today: Date) -> Result<Policy> {
    let policy = load_policy(root)?;
    validate_policy(&policy, today)?;
    let actual = fs::read_to_string(root.join("RULEBOOK.md")).map_err(|e| e.to_string())?;
    require(
        actual == render(&policy),
        "RULEBOOK.md differs from canonical rules; run render and review",
    )?;
    guide::check(root)?;
    let saved: Manifest = read_json(&root.join("policy/reading-manifest.json"))?;
    require(
        saved.schema_version == 1 && saved.documents == current_manifest(root)?.documents,
        "Reading manifest stale or incomplete; review changes then run refresh-manifest",
    )?;
    validate_links(root)?;
    Ok(policy)
}
fn check_disposition(
    c: &Check,
    evidence: &BTreeMap<String, String>,
    acceptance: bool,
) -> Result<()> {
    require(!c.id.trim().is_empty(), "Empty check ID")?;
    require(
        ["pass", "fail", "blocked", "not-applicable"].contains(&c.status.as_str()),
        format!("Invalid check status {}", c.id),
    )?;
    require(
        useful(&c.reason),
        format!("Missing concrete reason for {}", c.id),
    )?;
    require(
        !acceptance || c.status != "not-applicable",
        format!(
            "Acceptance condition {} cannot be waived as not-applicable",
            c.id
        ),
    )?;
    if c.status == "pass" {
        require(!c.evidence.is_empty(), format!("No evidence for {}", c.id))?;
    }
    for id in &c.evidence {
        require(
            evidence.get(id).is_some_and(|x| useful(x)),
            format!("Missing evidence {id} for {}", c.id),
        )?;
    }
    Ok(())
}

fn git_version_is_supported(output: &str) -> bool {
    let Some(version) = output.trim().strip_prefix("git version ") else {
        return false;
    };
    let mut components = version.split('.');
    let Some(major) = components.next().and_then(|part| part.parse::<u64>().ok()) else {
        return false;
    };
    let Some(minor) = components.next().and_then(|part| part.parse::<u64>().ok()) else {
        return false;
    };
    let Some(patch_component) = components.next() else {
        return false;
    };
    let patch_length = patch_component
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    let Some(patch) = patch_component
        .get(..patch_length)
        .and_then(|part| part.parse::<u64>().ok())
    else {
        return false;
    };
    let patch_suffix = &patch_component[patch_length..];
    let remaining: Vec<_> = components.collect();
    let supported_vendor_suffix = if patch_suffix.is_empty() {
        remaining.is_empty()
            || (remaining.len() == 2
                && remaining[0] == "windows"
                && remaining[1].parse::<u64>().is_ok())
    } else {
        remaining.is_empty() && is_apple_git_suffix(patch_suffix)
    };
    if !supported_vendor_suffix {
        return false;
    }
    (major, minor, patch) >= (2, 36, 0)
}

fn is_apple_git_suffix(suffix: &str) -> bool {
    let Some(build) = suffix
        .trim_start_matches(char::is_whitespace)
        .strip_prefix("(Apple Git-")
        .and_then(|suffix| suffix.strip_suffix(')'))
    else {
        return false;
    };
    !build.is_empty() && build.bytes().all(|byte| byte.is_ascii_digit())
}

fn require_supported_git(root: &Path) -> Result<()> {
    let version = git_command(root)
        .arg("--version")
        .output()
        .map_err(|_| "Cannot verify audit checkout state".to_string())?;
    require(
        version.status.success()
            && std::str::from_utf8(&version.stdout)
                .ok()
                .is_some_and(git_version_is_supported),
        "Audit requires Git 2.36.0 or newer",
    )
}

fn relative_git_path(path: &[u8]) -> Result<PathBuf> {
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(path.to_vec())
    };
    #[cfg(windows)]
    let path = OsString::from(
        std::str::from_utf8(path)
            .map_err(|_| "Audit requires a complete Git checkout".to_string())?,
    );
    #[cfg(not(any(unix, windows)))]
    let path = OsString::from(
        std::str::from_utf8(path)
            .map_err(|_| "Audit requires a complete Git checkout".to_string())?,
    );

    let path = PathBuf::from(path);
    require(
        !path.as_os_str().is_empty()
            && path.is_relative()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "Audit requires a complete Git checkout",
    )?;
    Ok(path)
}

struct IndexedFile {
    mode: String,
    object_id: String,
    path: PathBuf,
}

fn read_indexed_files(root: &Path) -> Result<Vec<IndexedFile>> {
    let index = git_command(root)
        .args([
            "-c",
            "core.fsmonitor=false",
            "--no-optional-locks",
            "ls-files",
            "--stage",
            "-z",
        ])
        .output()
        .map_err(|_| "Cannot verify audit checkout state".to_string())?;
    require(index.status.success(), "Cannot verify audit checkout state")?;

    let mut files = Vec::new();
    for entry in index
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let Some(separator) = entry.iter().position(|byte| *byte == b'\t') else {
            return Err("Audit requires a complete Git checkout".into());
        };
        let mut fields = entry[..separator].split(|byte| *byte == b' ');
        let Some(mode) = fields.next() else {
            return Err("Audit requires a complete Git checkout".into());
        };
        let Some(object_id) = fields.next() else {
            return Err("Audit requires a complete Git checkout".into());
        };
        let Some(stage) = fields.next() else {
            return Err("Audit requires a complete Git checkout".into());
        };
        require(
            fields.next().is_none()
                && matches!(mode, b"100644" | b"100755")
                && stage == b"0"
                && matches!(object_id.len(), 40 | 64)
                && object_id.iter().all(u8::is_ascii_hexdigit),
            "Audit requires a complete Git checkout",
        )?;
        let object_id = std::str::from_utf8(object_id)
            .map_err(|_| "Audit requires a complete Git checkout".to_string())?
            .to_owned();
        let mode = std::str::from_utf8(mode)
            .map_err(|_| "Audit requires a complete Git checkout".to_string())?
            .to_owned();
        let path = relative_git_path(&entry[separator + 1..])?;
        files.push(IndexedFile {
            mode,
            object_id,
            path,
        });
    }
    Ok(files)
}

// Keep the command line well below platform limits, including Windows.
const MAX_CHECK_ATTR_BATCH_ARGUMENT_SIZE: usize = 8 * 1024;

fn git_path_argument_size(path: &Path) -> usize {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().count()
    }
    #[cfg(not(windows))]
    {
        path.as_os_str().as_encoded_bytes().len()
    }
}

fn require_filter_attribute_is_unset(root: &Path, paths: &[&Path], cached: bool) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut command = git_command(root);
    command.args([
        "-c",
        "core.fsmonitor=false",
        "--no-optional-locks",
        "check-attr",
    ]);
    if cached {
        command.arg("--cached");
    }
    let attributes = command
        .args(["-z", "filter", "--"])
        .args(paths)
        .output()
        .map_err(|_| "Cannot verify audit checkout state".to_string())?;
    require(
        attributes.status.success(),
        "Cannot verify audit checkout state",
    )?;

    let mut fields = attributes.stdout.split(|byte| *byte == 0);
    for _ in paths {
        let path = fields
            .next()
            .ok_or_else(|| "Audit requires a complete Git checkout".to_string())?;
        let attribute = fields
            .next()
            .ok_or_else(|| "Audit requires a complete Git checkout".to_string())?;
        let value = fields
            .next()
            .ok_or_else(|| "Audit requires a complete Git checkout".to_string())?;
        require(
            !path.is_empty()
                && attribute == b"filter"
                && matches!(value, b"unspecified" | b"unset"),
            "Audit requires a complete Git checkout",
        )?;
    }
    let terminator = fields
        .next()
        .ok_or_else(|| "Audit requires a complete Git checkout".to_string())?;
    require(
        terminator.is_empty() && fields.next().is_none(),
        "Audit requires a complete Git checkout",
    )
}

fn require_no_active_clean_filters(root: &Path, files: &[IndexedFile]) -> Result<()> {
    for cached in [false, true] {
        let mut paths = Vec::new();
        let mut batch_size = 0usize;
        for file in files {
            let path = file.path.as_path();
            let path_size = git_path_argument_size(path).saturating_add(1);
            if !paths.is_empty()
                && batch_size.saturating_add(path_size) > MAX_CHECK_ATTR_BATCH_ARGUMENT_SIZE
            {
                require_filter_attribute_is_unset(root, &paths, cached)?;
                paths.clear();
                batch_size = 0;
            }
            if path_size > MAX_CHECK_ATTR_BATCH_ARGUMENT_SIZE {
                require_filter_attribute_is_unset(root, &[path], cached)?;
            } else {
                paths.push(path);
                batch_size = batch_size.saturating_add(path_size);
            }
        }
        require_filter_attribute_is_unset(root, &paths, cached)?;
    }
    Ok(())
}

fn require_index_content_matches_worktree(root: &Path, files: &[IndexedFile]) -> Result<()> {
    for file in files {
        let path = &file.path;
        let metadata = fs::symlink_metadata(root.join(path))
            .map_err(|_| "Audit requires a complete Git checkout".to_string())?;
        require(
            metadata.file_type().is_file(),
            "Audit requires a complete Git checkout",
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            require(
                (metadata.permissions().mode() & 0o111 != 0) == (file.mode == "100755"),
                "Audit requires a clean Git worktree",
            )?;
        }

        let mut path_option = OsString::from("--path=");
        path_option.push(path.as_os_str());
        let actual = git_command(root)
            .args([
                "-c",
                "core.fsmonitor=false",
                "--no-optional-locks",
                "hash-object",
            ])
            .arg(path_option)
            .arg("--")
            .arg(path)
            .output()
            .map_err(|_| "Cannot verify audit checkout state".to_string())?;
        require(
            actual.status.success(),
            "Cannot verify audit checkout state",
        )?;
        let actual = std::str::from_utf8(&actual.stdout)
            .map_err(|_| "Cannot verify audit checkout state".to_string())?
            .trim();
        require(
            actual == file.object_id,
            "Audit requires a clean Git worktree",
        )?;
    }
    Ok(())
}

// Source state covers tracked files and non-ignored untracked files. Ignored
// private data and build outputs intentionally remain outside this contract.
fn require_complete_audit_checkout(root: &Path) -> Result<()> {
    require_supported_git(root)?;
    // Force status to inspect the worktree instead of trusting FSMonitor or
    // untracked-cache entries that can hide source changes.
    let index = git_command(root)
        .args([
            "-c",
            "core.fsmonitor=false",
            "--no-optional-locks",
            "ls-files",
            "-v",
            "-z",
        ])
        .output()
        .map_err(|_| "Cannot verify audit checkout state".to_string())?;
    require(index.status.success(), "Cannot verify audit checkout state")?;
    // Lowercase tags mean assume-unchanged; S means skip-worktree. Both can
    // make ordinary `git status` omit tracked source changes.
    let hidden_tracked_files = index.stdout.split(|byte| *byte == 0).any(|entry| {
        entry
            .first()
            .is_some_and(|tag| tag.is_ascii_lowercase() || *tag == b'S')
    });
    require(
        !hidden_tracked_files,
        "Audit requires a complete Git checkout",
    )
}

fn require_clean_audit_worktree(root: &Path) -> Result<()> {
    require_complete_audit_checkout(root)?;
    let indexed_files = read_indexed_files(root)?;
    // Check both attribute views before either status or hash-object can apply
    // an external clean filter.
    require_no_active_clean_filters(root, &indexed_files)?;
    let status = git_command(root)
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ])
        .output()
        .map_err(|_| "Cannot verify audit worktree state".to_string())?;
    require(
        status.status.success(),
        "Cannot verify audit worktree state",
    )?;
    // Status can contain private paths. Only inspect whether it is empty.
    require(
        status.stdout.is_empty(),
        "Audit requires a clean Git worktree",
    )?;
    require_index_content_matches_worktree(root, &indexed_files)
}

fn require_audit_head(root: &Path, revision: &str) -> Result<()> {
    let head = git_command(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|_| "Cannot identify repository HEAD".to_string())?;
    require(
        head.status.success() && String::from_utf8_lossy(&head.stdout).trim() == revision,
        "Audit revision does not match checkout HEAD",
    )
}

pub fn validate_audit(root: &Path, policy: &Policy, a: &Audit, now: OffsetDateTime) -> Result<()> {
    require_clean_audit_worktree(root)?;
    require(a.schema_version == 1, "Unsupported audit schema")?;
    require(useful(&a.task), "Task missing or placeholder")?;
    require(a.outcome == "complete", "Audit outcome is not complete")?;
    let recorded = OffsetDateTime::parse(&a.recorded_at, &Rfc3339)
        .map_err(|e| format!("Invalid audit timestamp: {e}"))?;
    require(
        recorded.offset().is_utc() && recorded <= now,
        "Audit timestamp must be UTC and not in future",
    )?;
    require(
        a.repository_revision.len() == 40
            && a.repository_revision.bytes().all(|c| c.is_ascii_hexdigit()),
        "Expected full Git revision",
    )?;
    // Bind the declared revision to this checkout, not an arbitrary plausible SHA.
    require_audit_head(root, &a.repository_revision)?;
    let expected = current_manifest(root)?;
    let mut seen = BTreeSet::new();
    for reading in &a.reading {
        require(
            seen.insert(reading.path.clone()),
            format!("Duplicate reading {}", reading.path),
        )?;
        require(
            expected.documents.get(&reading.path) == Some(&reading.sha256),
            format!("Stale or unknown reading {}", reading.path),
        )?;
        require(
            useful(&reading.note),
            format!("Missing application note for {}", reading.path),
        )?;
    }
    require(
        seen == expected.documents.keys().cloned().collect(),
        "Incomplete reading receipts",
    )?;
    for (id, observation) in &a.evidence {
        require(
            !id.trim().is_empty() && useful(observation),
            format!("Incomplete evidence {id}"),
        )?;
    }
    let mut ids: BTreeSet<_> = policy
        .rules
        .iter()
        .filter(|r| r.status == "active")
        .map(|r| r.id.clone())
        .collect();
    ids.extend(guide::module_ids(root)?);
    let mut checked = BTreeSet::new();
    for c in &a.checks {
        require(
            checked.insert(c.id.clone()) && ids.contains(&c.id),
            format!("Duplicate or unknown rule check {}", c.id),
        )?;
        check_disposition(c, &a.evidence, false)?;
        require(
            c.status != "not-applicable"
                || !policy.rules.iter().any(|r| r.id == c.id)
                || policy
                    .rules
                    .iter()
                    .any(|r| r.id == c.id && r.scope != "global"),
            format!("Global rule {} cannot be not-applicable", c.id),
        )?;
        require(
            ["pass", "not-applicable"].contains(&c.status.as_str()),
            format!("Unresolved rule {}: {}", c.id, c.status),
        )?;
    }
    require(checked == ids, "Missing rule dispositions")?;
    require(!a.acceptance.is_empty(), "No acceptance conditions")?;
    let mut acceptance_ids = BTreeSet::new();
    for c in &a.acceptance {
        require(
            acceptance_ids.insert(&c.id),
            "Duplicate acceptance condition",
        )?;
        check_disposition(c, &a.evidence, true)?;
        require(
            c.status == "pass",
            format!("Unresolved acceptance condition {}", c.id),
        )?;
    }
    require_audit_head(root, &a.repository_revision)?;
    require_clean_audit_worktree(root)
}
pub fn new_audit(root: &Path, task: String) -> Result<Audit> {
    require_clean_audit_worktree(root)?;
    let policy = load_policy(root)?;
    let head = git_command(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|_| "Cannot identify repository HEAD".to_string())?;
    require(head.status.success(), "Cannot identify repository HEAD")?;
    let audit = Audit {
        schema_version: 1,
        task,
        outcome: "in-progress".into(),
        recorded_at: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|e| e.to_string())?,
        repository_revision: String::from_utf8_lossy(&head.stdout).trim().into(),
        reading: current_manifest(root)?
            .documents
            .into_iter()
            .map(|(path, sha256)| Reading {
                path,
                sha256,
                note: "UNFILLED: read and record concrete applicability".into(),
            })
            .collect(),
        evidence: BTreeMap::new(),
        checks: policy
            .rules
            .iter()
            .filter(|r| r.status == "active")
            .map(|r| Check {
                id: r.id.clone(),
                status: "blocked".into(),
                reason: "UNFILLED: inspect applicability and evidence".into(),
                evidence: vec![],
            })
            .chain(guide::module_ids(root)?.into_iter().map(|id| Check {
                id,
                status: "blocked".into(),
                reason: "UNFILLED: inspect scoped module rule and evidence".into(),
                evidence: vec![],
            }))
            .collect(),
        acceptance: vec![Check {
            id: "AC-01".into(),
            status: "blocked".into(),
            reason: "UNFILLED: replace with actual task acceptance conditions".into(),
            evidence: vec![],
        }],
    };
    require_audit_head(root, &audit.repository_revision)?;
    require_clean_audit_worktree(root)?;
    Ok(audit)
}

#[cfg(test)]
mod git_version_tests {
    use super::git_version_is_supported;

    #[test]
    fn audit_git_version_gate_accepts_supported_releases_and_fails_closed() {
        assert!(git_version_is_supported("git version 2.36.0"));
        assert!(git_version_is_supported("git version 2.53.0.windows.3"));
        assert!(git_version_is_supported(
            "git version 2.39.5 (Apple Git-154)"
        ));
        assert!(!git_version_is_supported("git version 2.35.9"));
        assert!(!git_version_is_supported("git version 2.36.0-rc1"));
        assert!(!git_version_is_supported("git version 2.36.0.rc1"));
        assert!(!git_version_is_supported("unrecognized version"));
    }
}
