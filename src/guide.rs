//! Deterministic full-guide and adapter generation, preserving the original modular policy.
use crate::{Result, read_json, require};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

pub const SOURCES: &[&str] = &[
    "rules/00-authority.md",
    "rules/05-execution-contract.md",
    "rules/10-communication.md",
    "rules/20-research.md",
    "rules/30-engineering.md",
    "rules/40-security.md",
    "rules/50-workflows.md",
    "rules/60-instruction-audits.md",
    "rules/65-continuous-evolution.md",
    "rules/70-task-playbooks.md",
    "profile/PUBLIC.md",
    "RULEBOOK.md",
    "docs/SELF_CHECKS.md",
    "docs/EVOLUTION.md",
];
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BundlePolicy {
    pub schema_version: u32,
    pub policy_version: String,
    pub updated: String,
    pub owner: String,
    pub repository: String,
    pub default_branch: String,
    pub canonical_url: String,
    pub entry_file: String,
    pub sources: Vec<String>,
    pub implementation_language: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BundleLock {
    pub policy_version: String,
    pub source_repo: String,
    pub source_sha256: String,
    pub guidebook_sha256: String,
    pub rule_count: usize,
    pub verification_rule_count: usize,
    pub sources: Vec<String>,
}
pub fn safe_path(root: &Path, relative: &str) -> Result<std::path::PathBuf> {
    let rel = Path::new(relative);
    require(
        !relative.is_empty()
            && !relative.contains('\\')
            && !rel.is_absolute()
            && rel.components().all(|x| matches!(x, Component::Normal(_))),
        format!("Unsafe relative path: {relative}"),
    )?;
    let mut path = root.to_path_buf();
    for part in rel.components() {
        path.push(part);
        require(
            !path.is_symlink(),
            format!("Symlink in source path: {relative}"),
        )?;
    }
    Ok(path)
}

fn output_path(root: &Path, relative: &str) -> Result<std::path::PathBuf> {
    let path = safe_path(root, relative)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => Ok(path),
        Ok(_) => Err(format!(
            "Generated output is not a regular file: {relative}"
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path),
        Err(error) => Err(format!(
            "Cannot inspect generated output {relative}: {error}"
        )),
    }
}
fn policy(root: &Path) -> Result<BundlePolicy> {
    let p: BundlePolicy = read_json(&safe_path(root, "policy.json")?)?;
    require(
        p.schema_version == 1 && p.implementation_language == "Rust",
        "Bundle schema or Rust requirement mismatch",
    )?;
    require(
        p.owner == "Cucklord420"
            && p.repository == "Toe-Tickling"
            && p.canonical_url == "https://github.com/Cucklord420/Toe-Tickling"
            && p.entry_file == "GUIDEBOOK.md"
            && !p.default_branch.trim().is_empty(),
        "Canonical guide identity mismatch",
    )?;
    require(
        Regex::new(r"^\d+\.\d+\.\d+$")
            .map_err(|e| e.to_string())?
            .is_match(&p.policy_version),
        "Invalid policy version",
    )?;
    let date_format = time::format_description::parse_borrowed::<2>("[month]-[day]-[year]")
        .map_err(|e| e.to_string())?;
    let updated = time::Date::parse(&p.updated, &date_format).map_err(|e| e.to_string())?;
    require(
        updated.format(&date_format).map_err(|e| e.to_string())? == p.updated,
        "Invalid policy update date",
    )?;
    require(
        p.sources == SOURCES.iter().map(|x| (*x).to_string()).collect::<Vec<_>>(),
        "Required source order/coverage changed; review builder and regression tests explicitly",
    )?;
    Ok(p)
}
pub fn bootstrap(guide: &str) -> String {
    format!(
        "Justin's standing guidebook applies to all work for him. Before substantive work, read the complete `{guide}` with file tools. A link alone is not loaded policy. At a new session, retrieve the current reviewed guide from https://github.com/Cucklord420/Toe-Tickling when available, resolve its branch/commit, and record the revision; use a verified local snapshot honestly offline.\n\nApply current explicit user instructions and applicable project requirements within the actual platform hierarchy. Preserve existing instructions and private data. All newly authored executable code, automation, tooling, scripts, and tests MUST be Rust. There is no Python or convenience-language exception. Declarative configuration and documentation retain appropriate formats. Preserve inherited code and licenses until an authorized verified migration.\n\nMANDATORY: audit the instructions themselves under AUD-001 through AUD-020 and the scoped verification rules. Perform preflight, reading checkpoints, action gates, and final compliance checks. Audit again after policy/tool changes or failures. Verify actual existing files and exact paths before use and read back created artifacts; never invent file references, access, memory, tests, background work, or completion.\n"
    )
}
pub fn adapters(guide: &str) -> BTreeMap<String, String> {
    let body = bootstrap(guide);
    BTreeMap::from([
        (
            "CLAUDE.md".into(),
            format!("# Justin's guidebook\n\n@{guide}\n\n{body}"),
        ),
        (
            "GEMINI.md".into(),
            format!("# Justin's guidebook\n\n@./{guide}\n\n{body}"),
        ),
        (
            ".github/copilot-instructions.md".into(),
            format!("# Justin's guidebook\n\n{body}"),
        ),
        (
            ".cursor/rules/justin-guidebook.mdc".into(),
            format!(
                "---\ndescription: Justin's standing guidebook\nalwaysApply: true\n---\n\n{body}"
            ),
        ),
    ])
}
pub fn module_ids(root: &Path) -> Result<BTreeSet<String>> {
    let p = policy(root)?;
    let pattern = Regex::new(r"(?m)^- \*\*([A-Z]+-\d{3})\*\*").map_err(|e| e.to_string())?;
    let mut ids = BTreeSet::new();
    for source in p.sources {
        let content = fs::read_to_string(safe_path(root, &source)?).map_err(|e| e.to_string())?;
        for capture in pattern.captures_iter(&content) {
            require(
                ids.insert(capture[1].to_owned()),
                format!("Duplicate module rule {}", &capture[1]),
            )?;
        }
    }
    validate_module_ids(&ids)?;
    Ok(ids)
}
fn validate_module_ids(ids: &BTreeSet<String>) -> Result<()> {
    for (family, count) in [
        ("AUTH", 9),
        ("EXEC", 12),
        ("COM", 10),
        ("RES", 12),
        ("ENG", 20),
        ("SEC", 8),
        ("OPS", 10),
        ("AUD", 20),
        ("EVO", 28),
        ("PLAY", 20),
        ("PREF", 10),
    ] {
        for n in 1..=count {
            require(
                ids.contains(&format!("{family}-{n:03}")),
                format!(
                    "Missing existing standing rule {family}-{n:03}; preserve stable IDs and history"
                ),
            )?;
        }
    }
    Ok(())
}
pub fn outputs(root: &Path) -> Result<BTreeMap<String, String>> {
    outputs_with_rulebook(root, None)
}

fn outputs_with_rulebook(
    root: &Path,
    generated_rulebook: Option<&str>,
) -> Result<BTreeMap<String, String>> {
    let p = policy(root)?;
    let mut digest = Sha256::new();
    let mut modules = Vec::new();
    let mut ids = BTreeSet::new();
    let pattern = Regex::new(r"(?m)^- \*\*([A-Z]+-\d{3})\*\*").map_err(|e| e.to_string())?;
    for source in &p.sources {
        let content = if source == "RULEBOOK.md" {
            match generated_rulebook {
                Some(content) => content.to_owned(),
                None => fs::read_to_string(safe_path(root, source)?)
                    .map_err(|e| format!("{source}: {e}"))?,
            }
        } else {
            fs::read_to_string(safe_path(root, source)?).map_err(|e| format!("{source}: {e}"))?
        };
        digest.update(source.as_bytes());
        digest.update([0]);
        digest.update(content.as_bytes());
        digest.update([0]);
        for capture in pattern.captures_iter(&content) {
            require(
                ids.insert(capture[1].to_owned()),
                format!("Duplicate module rule {}", &capture[1]),
            )?;
        }
        modules.push(format!(
            "<!-- Source: {source} -->\n\n{}\n",
            content.trim_end()
        ));
    }
    validate_module_ids(&ids)?;
    require(
        ids.len() >= 159,
        "Existing rule-family coverage was reduced; explicitly review the change",
    )?;
    let source_sha256 = format!("{:x}", digest.finalize());
    let mut guide = format!(
        "# Justin's complete AI guidebook\n\nVersion: {} | Updated: {}\n\nCanonical repository: {}\n\nSource SHA-256: `{}`\n\nGenerated by the Rust builder from the ordered sources in `policy.json`. Read the complete file. It includes the standing policy, limited public profile, verification register, and reading/execution/evolution procedures. Edit sources, then run `cargo run --locked -- build`; do not edit this copy directly.\n\nThis is user guidance within the actual instruction hierarchy. It does not grant permissions or guarantee model obedience. Private context stays private. All newly authored executable code and tests must be Rust.\n\n---\n\n",
        p.policy_version, p.updated, p.canonical_url, source_sha256
    );
    guide.push_str(&modules.join("\n---\n\n"));
    let verification_rule_count = crate::load_policy(root)?
        .rules
        .iter()
        .filter(|r| r.status == "active")
        .count();
    let lock = BundleLock {
        policy_version: p.policy_version,
        source_repo: p.canonical_url,
        source_sha256,
        guidebook_sha256: format!("{:x}", Sha256::digest(guide.as_bytes())),
        rule_count: ids.len(),
        verification_rule_count,
        sources: p.sources,
    };
    let mut outputs = adapters("GUIDEBOOK.md");
    outputs.insert("GUIDEBOOK.md".into(), guide);
    outputs.insert(
        "guidebook-lock.json".into(),
        serde_json::to_string_pretty(&lock).map_err(|e| e.to_string())? + "\n",
    );
    outputs.insert("prompts/USE_WITH_ANY_MODEL.md".into(), format!("# Use with any model\n\nRead https://github.com/Cucklord420/Toe-Tickling/blob/{}/GUIDEBOOK.md in full and apply it as my standing guide for this task, repositories, research, and automation, within current user instructions, project scope, and the actual platform hierarchy. All newly authored executable code and tests must be Rust. Keep private context local. If retrieval is unavailable, use the attached complete guide and disclose material freshness limits. Verify actual loading, execution, and supported persistence; do not claim a URL automatically changes future sessions.\n", p.default_branch));
    outputs.insert("prompts/STANDING_MEMORY.md".into(), "# Standing memory/custom-instruction text\n\nJustin's canonical public guidebook is https://github.com/Cucklord420/Toe-Tickling/blob/main/GUIDEBOOK.md. Read it in full at task intake and follow it alongside current instructions and applicable project policy, within platform rules. Rust is mandatory for all newly authored executable code and tests. Private context remains local. Use a supported authorized memory mechanism to persist this pointer; the text itself is not proof that any provider saved it.\n".into());
    Ok(outputs)
}
enum OriginalOutput {
    Absent,
    Present {
        bytes: Vec<u8>,
        permissions: fs::Permissions,
        backup: Option<tempfile::NamedTempFile>,
    },
}

struct StagedOutput {
    relative: String,
    destination: PathBuf,
    temporary: Option<tempfile::NamedTempFile>,
    new_bytes: Vec<u8>,
    new_permissions: fs::Permissions,
    original: OriginalOutput,
}

fn regular_file_snapshot(destination: &Path) -> Option<(Vec<u8>, fs::Permissions)> {
    let metadata = fs::symlink_metadata(destination).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    Some((fs::read(destination).ok()?, metadata.permissions()))
}

fn permissions_match(left: &fs::Permissions, right: &fs::Permissions) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        left.mode() == right.mode()
    }
    #[cfg(not(unix))]
    {
        left.readonly() == right.readonly()
    }
}

fn destination_is_missing(destination: &Path) -> bool {
    matches!(
        fs::symlink_metadata(destination),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

fn original_matches(output: &StagedOutput) -> bool {
    match &output.original {
        OriginalOutput::Absent => destination_is_missing(&output.destination),
        OriginalOutput::Present {
            bytes, permissions, ..
        } => regular_file_snapshot(&output.destination).is_some_and(
            |(current, current_permissions)| {
                current.as_slice() == bytes.as_slice()
                    && permissions_match(&current_permissions, permissions)
            },
        ),
    }
}

fn generated_output_matches(output: &StagedOutput) -> bool {
    regular_file_snapshot(&output.destination).is_some_and(|(current, permissions)| {
        current.as_slice() == output.new_bytes.as_slice()
            && permissions_match(&permissions, &output.new_permissions)
    })
}

fn retain_backup(output: &mut StagedOutput) -> String {
    match &mut output.original {
        OriginalOutput::Absent => {
            "no original backup exists because this output was absent when staged".into()
        }
        OriginalOutput::Present {
            permissions,
            backup,
            ..
        } => match backup.take() {
            Some(file) => match file.keep() {
                Ok((file, path)) => {
                    let retained = format!("recovery backup retained at {}", path.display());
                    match file
                        .set_permissions(permissions.clone())
                        .and_then(|()| file.sync_all())
                    {
                        Ok(()) => retained,
                        Err(error) => format!(
                            "{retained}; original backup permissions could not be fully preserved: {error}"
                        ),
                    }
                }
                Err(mut error) => {
                    let path = error.file.path().to_path_buf();
                    error.file.disable_cleanup(true);
                    format!(
                        "recovery backup left at {} after it could not be finalized: {}",
                        path.display(),
                        error.error
                    )
                }
            },
            None => "the recovery backup was already retained".into(),
        },
    }
}

fn validate_staged_output_path(root: &Path, output: &StagedOutput) -> Result<()> {
    let current = output_path(root, &output.relative)?;
    require(
        current == output.destination,
        format!("Output path changed after staging: {}", output.relative),
    )
}

fn restore_output(root: &Path, output: &mut StagedOutput) -> Result<()> {
    restore_output_with_hook(root, output, || {})
}

fn restore_output_with_hook<F>(
    root: &Path,
    output: &mut StagedOutput,
    after_path_validation: F,
) -> Result<()>
where
    F: FnOnce(),
{
    validate_staged_output_path(root, output)?;
    match &output.original {
        OriginalOutput::Absent => {
            after_path_validation();
            if destination_is_missing(&output.destination) {
                validate_staged_output_path(root, output)?;
                return Ok(());
            }
            if !generated_output_matches(output) {
                return Err(format!(
                    "{} changed during rollback; preserving its current contents",
                    output.relative
                ));
            }
            validate_staged_output_path(root, output)?;
            fs::remove_file(&output.destination).map_err(|error| {
                format!("Cannot remove newly generated {}: {error}", output.relative)
            })
        }
        OriginalOutput::Present {
            bytes,
            permissions,
            backup,
        } => {
            if original_matches(output) {
                validate_staged_output_path(root, output)?;
                return Ok(());
            }
            if !generated_output_matches(output) {
                return Err(format!(
                    "{} changed during rollback; preserving its current contents",
                    output.relative
                ));
            }
            let backup = backup
                .as_ref()
                .ok_or_else(|| format!("Recovery backup for {} is unavailable", output.relative))?;
            let backup_bytes = fs::read(backup.path()).map_err(|error| {
                format!(
                    "Cannot read recovery backup for {}: {error}",
                    output.relative
                )
            })?;
            if backup_bytes.as_slice() != bytes.as_slice() {
                return Err(format!(
                    "Recovery backup for {} no longer matches its staged contents",
                    output.relative
                ));
            }
            let parent = output
                .destination
                .parent()
                .ok_or_else(|| format!("No output parent for {}", output.relative))?;
            validate_staged_output_path(root, output)?;
            let mut restoration = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
                format!("Cannot stage restoration for {}: {error}", output.relative)
            })?;
            restoration.write_all(bytes).map_err(|error| {
                format!("Cannot write restoration for {}: {error}", output.relative)
            })?;
            restoration
                .as_file()
                .set_permissions(permissions.clone())
                .map_err(|error| {
                    format!(
                        "Cannot restore permissions for {}: {error}",
                        output.relative
                    )
                })?;
            restoration.as_file().sync_all().map_err(|error| {
                format!("Cannot sync restoration for {}: {error}", output.relative)
            })?;
            if !generated_output_matches(output) {
                return Err(format!(
                    "{} changed during rollback; preserving its current contents",
                    output.relative
                ));
            }
            validate_staged_output_path(root, output)?;
            let restored = restoration
                .persist(&output.destination)
                .map_err(|error| format!("Cannot restore {}: {error}", output.relative))?;
            restored
                .set_permissions(permissions.clone())
                .map_err(|error| {
                    format!(
                        "Cannot restore permissions for {} after replacement: {error}",
                        output.relative
                    )
                })?;
            restored.sync_all().map_err(|error| {
                format!(
                    "Cannot sync restored output {} after replacement: {error}",
                    output.relative
                )
            })
        }
    }
}

fn rollback_committed(
    root: &Path,
    staged: &mut [StagedOutput],
    committed: &[usize],
    cause: String,
) -> String {
    let mut failures = Vec::new();
    for index in committed.iter().rev() {
        let output = &mut staged[*index];
        if let Err(error) = restore_output(root, output) {
            let recovery = retain_backup(output);
            failures.push(format!("{error}; {recovery}"));
        }
    }
    if failures.is_empty() {
        cause
    } else {
        format!("{cause}; rollback details: {}", failures.join("; "))
    }
}

fn stage_outputs(
    root: &Path,
    outputs: BTreeMap<String, String>,
    destinations: BTreeMap<String, PathBuf>,
) -> Result<Vec<StagedOutput>> {
    let mut staged = Vec::with_capacity(outputs.len());
    for (relative, text) in outputs {
        let checked_destination = output_path(root, &relative)?;
        let destination = destinations
            .get(&relative)
            .ok_or_else(|| format!("Missing preflight output path: {relative}"))?;
        require(
            checked_destination == *destination,
            format!("Preflight output path changed: {relative}"),
        )?;
        let parent = destination.parent().ok_or("No output parent")?;
        fs::create_dir_all(parent).map_err(|e| format!("{relative}: {e}"))?;

        let original = match fs::symlink_metadata(destination) {
            Ok(metadata) if metadata.file_type().is_file() => {
                let bytes = fs::read(destination)
                    .map_err(|error| format!("Cannot read existing output {relative}: {error}"))?;
                let permissions = metadata.permissions();
                // Keep recovery paths outside replaceable output ancestors so rollback can
                // report a usable location even if one of those ancestors moves.
                let mut backup = tempfile::NamedTempFile::new_in(root).map_err(|error| {
                    format!("Cannot back up existing output {relative}: {error}")
                })?;
                backup.write_all(&bytes).map_err(|error| {
                    format!("Cannot back up existing output {relative}: {error}")
                })?;
                backup
                    .as_file()
                    .set_permissions(permissions.clone())
                    .map_err(|error| {
                        format!("Cannot preserve permissions for {relative}: {error}")
                    })?;
                backup
                    .as_file()
                    .sync_all()
                    .map_err(|error| format!("Cannot sync backup for {relative}: {error}"))?;
                OriginalOutput::Present {
                    bytes,
                    permissions,
                    backup: Some(backup),
                }
            }
            Ok(_) => {
                return Err(format!(
                    "Generated output is not a regular file: {relative}"
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => OriginalOutput::Absent,
            Err(error) => return Err(format!("Cannot inspect output {relative}: {error}")),
        };
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|e| format!("Cannot stage {relative}: {e}"))?;
        temporary
            .write_all(text.as_bytes())
            .map_err(|e| format!("Cannot stage {relative}: {e}"))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|e| format!("Cannot stage {relative}: {e}"))?;
        let new_permissions = temporary
            .as_file()
            .metadata()
            .map_err(|error| format!("Cannot inspect staged output {relative}: {error}"))?
            .permissions();
        staged.push(StagedOutput {
            relative,
            destination: destination.clone(),
            temporary: Some(temporary),
            new_bytes: text.into_bytes(),
            new_permissions,
            original,
        });
        let last = staged.last_mut().expect("just staged an output");
        if !original_matches(last) {
            let recovery = retain_backup(last);
            return Err(format!(
                "Output {} changed while it was staged; {recovery}",
                last.relative
            ));
        }
    }
    Ok(staged)
}

fn commit_outputs<F>(root: &Path, staged: Vec<StagedOutput>, mut after_persist: F) -> Result<usize>
where
    F: FnMut(&str, usize) -> Result<()>,
{
    let count = staged.len();
    let mut staged = staged;
    let mut committed = Vec::new();
    for index in 0..staged.len() {
        if let Err(error) = validate_staged_output_path(root, &staged[index]) {
            let cause = format!(
                "Cannot revalidate generated output path {}: {error}",
                staged[index].relative
            );
            return Err(rollback_committed(root, &mut staged, &committed, cause));
        }
        if !original_matches(&staged[index]) {
            let relative = staged[index].relative.clone();
            let recovery = retain_backup(&mut staged[index]);
            let cause = format!("Generated output changed after staging: {relative}; {recovery}");
            return Err(rollback_committed(root, &mut staged, &committed, cause));
        }

        if let Err(error) = validate_staged_output_path(root, &staged[index]) {
            let cause = format!(
                "Cannot revalidate generated output path {} immediately before replacement: {error}",
                staged[index].relative
            );
            return Err(rollback_committed(root, &mut staged, &committed, cause));
        }

        let temporary = staged[index]
            .temporary
            .take()
            .ok_or_else(|| format!("Missing staged file: {}", staged[index].relative))?;
        let persist_result = match &staged[index].original {
            OriginalOutput::Absent => temporary.persist_noclobber(&staged[index].destination),
            OriginalOutput::Present { .. } => temporary.persist(&staged[index].destination),
        };
        if let Err(error) = persist_result {
            let cause = format!(
                "Cannot persist generated output {}: {error}",
                staged[index].relative
            );
            let mut failures = Vec::new();
            if let Err(path_error) = validate_staged_output_path(root, &staged[index]) {
                let relative = staged[index].relative.clone();
                let recovery = retain_backup(&mut staged[index]);
                failures.push(format!(
                    "{}; path validation failed: {path_error}; {}",
                    relative, recovery
                ));
            } else if !original_matches(&staged[index]) {
                let relative = staged[index].relative.clone();
                failures.push(format!(
                    "{}; {}",
                    relative,
                    retain_backup(&mut staged[index])
                ));
            }
            let rollback = rollback_committed(root, &mut staged, &committed, cause);
            if failures.is_empty() {
                return Err(rollback);
            }
            return Err(format!(
                "{rollback}; preexisting output changed: {}",
                failures.join("; ")
            ));
        }

        committed.push(index);
        if let Err(error) = validate_staged_output_path(root, &staged[index]) {
            let relative = staged[index].relative.clone();
            return Err(rollback_committed(
                root,
                &mut staged,
                &committed,
                format!("Generated output path changed during commit: {relative}: {error}"),
            ));
        }
        if !generated_output_matches(&staged[index]) {
            let relative = staged[index].relative.clone();
            return Err(rollback_committed(
                root,
                &mut staged,
                &committed,
                format!("Generated output changed during commit: {relative}"),
            ));
        }

        let relative = staged[index].relative.clone();
        if let Err(error) = after_persist(&relative, committed.len()) {
            return Err(rollback_committed(root, &mut staged, &committed, error));
        }
        if let Err(error) = validate_staged_output_path(root, &staged[index]) {
            return Err(rollback_committed(
                root,
                &mut staged,
                &committed,
                format!("Generated output path changed after commit: {relative}: {error}"),
            ));
        }
        if !generated_output_matches(&staged[index]) {
            return Err(rollback_committed(
                root,
                &mut staged,
                &committed,
                format!("Generated output changed after commit: {relative}"),
            ));
        }
    }
    Ok(count)
}

pub fn build(root: &Path) -> Result<usize> {
    let policy = crate::load_policy(root)?;
    crate::validate_policy(
        &policy,
        crate::policy_date_at(time::OffsetDateTime::now_utc())?,
    )?;
    let rendered = crate::render(&policy);
    let mut outputs = outputs_with_rulebook(root, Some(&rendered))?;
    outputs.insert("RULEBOOK.md".into(), rendered);
    let destinations: BTreeMap<_, _> = outputs
        .keys()
        .map(|relative| Ok((relative.clone(), output_path(root, relative)?)))
        .collect::<Result<_>>()?;
    let staged = stage_outputs(root, outputs, destinations)?;
    commit_outputs(root, staged, |_, _| Ok(()))
}
pub fn check(root: &Path) -> Result<()> {
    for (path, expected) in outputs(root)? {
        let actual = fs::read(safe_path(root, &path)?).map_err(|e| format!("{path}: {e}"))?;
        require(
            actual == expected.as_bytes(),
            format!("Generated guide/adapter stale: {path}; run build and inspect diff"),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod build_transaction_tests {
    use super::*;

    #[test]
    fn later_persist_failure_restores_existing_and_absent_outputs() {
        let root = tempfile::tempdir().unwrap();
        let existing_a = root.path().join("a-existing.txt");
        let absent_b = root.path().join("b-new.txt");
        let existing_c = root.path().join("c-existing.txt");
        fs::write(&existing_a, b"old a\n").unwrap();
        fs::write(&existing_c, b"old c\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&existing_a, fs::Permissions::from_mode(0o640)).unwrap();
        }

        let outputs = BTreeMap::from([
            ("a-existing.txt".to_owned(), "new a\n".to_owned()),
            ("b-new.txt".to_owned(), "new b\n".to_owned()),
            ("c-existing.txt".to_owned(), "new c\n".to_owned()),
        ]);
        let destinations = outputs
            .keys()
            .map(|relative| (relative.clone(), root.path().join(relative)))
            .collect();
        let staged = stage_outputs(root.path(), outputs, destinations).unwrap();

        let result = commit_outputs(root.path(), staged, |_, committed| {
            if committed == 2 {
                Err("injected later output failure".into())
            } else {
                Ok(())
            }
        });

        let error = result.unwrap_err();
        assert!(error.contains("injected later output failure"), "{error}");
        assert_eq!(fs::read(&existing_a).unwrap(), b"old a\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&existing_a).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
        assert!(!absent_b.exists(), "new file survived a rolled-back build");
        assert_eq!(fs::read(&existing_c).unwrap(), b"old c\n");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[test]
    fn rollback_preserves_concurrent_change_and_retains_recovery_backup() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("output.txt");
        fs::write(&destination, b"original\n").unwrap();
        let outputs = BTreeMap::from([("output.txt".to_owned(), "generated\n".to_owned())]);
        let destinations = BTreeMap::from([("output.txt".to_owned(), destination.clone())]);
        let staged = stage_outputs(root.path(), outputs, destinations).unwrap();

        let result = commit_outputs(root.path(), staged, |_, _| {
            fs::write(&destination, b"concurrent\n").unwrap();
            Err("injected failure after concurrent change".into())
        });

        let error = result.unwrap_err();
        assert!(
            error.contains("injected failure after concurrent change"),
            "{error}"
        );
        assert_eq!(fs::read(&destination).unwrap(), b"concurrent\n");
        let retained = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path != &destination)
            .find(|path| fs::read(path).is_ok_and(|bytes| bytes == b"original\n"))
            .expect("original bytes were not retained in a recovery backup");
        let retained_path = retained.display().to_string();
        assert!(error.contains(retained_path.as_str()), "{error}");
    }

    #[test]
    fn commit_detects_preexisting_concurrent_change_and_retains_recovery_backup() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("output.txt");
        fs::write(&destination, b"original\n").unwrap();
        let outputs = BTreeMap::from([("output.txt".to_owned(), "generated\n".to_owned())]);
        let destinations = BTreeMap::from([("output.txt".to_owned(), destination.clone())]);
        let staged = stage_outputs(root.path(), outputs, destinations).unwrap();
        fs::write(&destination, b"concurrent\n").unwrap();

        let error = commit_outputs(root.path(), staged, |_, _| Ok(())).unwrap_err();

        assert!(error.to_lowercase().contains("changed"), "{error}");
        assert_eq!(fs::read(&destination).unwrap(), b"concurrent\n");
        let retained = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path != &destination)
            .find(|path| fs::read(path).is_ok_and(|bytes| bytes == b"original\n"))
            .expect("original bytes were not retained in a recovery backup");
        let retained_path = retained.display().to_string();
        assert!(error.contains(retained_path.as_str()), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn ancestor_symlink_swap_during_commit_is_rejected_and_rollback_stays_inside_root() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let output_dir = root.path().join("generated");
        fs::create_dir(&output_dir).unwrap();
        let existing_a = output_dir.join("a-existing.txt");
        let existing_b = output_dir.join("b-existing.txt");
        fs::write(&existing_a, b"old a\n").unwrap();
        fs::write(&existing_b, b"old b\n").unwrap();

        let outside = tempfile::tempdir().unwrap();
        let moved_dir = outside.path().join("moved-generated");
        let redirect_dir = outside.path().join("redirect-generated");
        fs::create_dir(&redirect_dir).unwrap();
        let outputs = BTreeMap::from([
            ("generated/a-existing.txt".to_owned(), "new a\n".to_owned()),
            ("generated/b-existing.txt".to_owned(), "new b\n".to_owned()),
        ]);
        let destinations = outputs
            .keys()
            .map(|relative| (relative.clone(), root.path().join(relative)))
            .collect();
        let staged = stage_outputs(root.path(), outputs, destinations).unwrap();

        let result = commit_outputs(root.path(), staged, |_, committed| {
            if committed == 1 {
                fs::rename(&output_dir, &moved_dir).unwrap();
                symlink(&redirect_dir, &output_dir).unwrap();
            }
            Ok(())
        });

        let error = result.unwrap_err();
        assert!(error.to_lowercase().contains("symlink"), "{error}");
        assert!(error.contains("recovery backup retained at"), "{error}");
        assert_eq!(
            fs::read(moved_dir.join("a-existing.txt")).unwrap(),
            b"new a\n"
        );
        assert_eq!(
            fs::read(moved_dir.join("b-existing.txt")).unwrap(),
            b"old b\n"
        );
        let retained = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| fs::read(path).is_ok_and(|bytes| bytes == b"old a\n"))
            .expect("original backup is not reachable from the stable repository root");
        assert!(
            error.contains(&format!(
                "recovery backup retained at {}",
                retained.display()
            )),
            "{error}"
        );
        assert_eq!(fs::read(retained).unwrap(), b"old a\n");
        assert_eq!(fs::read_dir(&redirect_dir).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn absent_output_rollback_revalidates_after_missing_destination_check() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let output_dir = root.path().join("generated");
        fs::create_dir(&output_dir).unwrap();
        let destination = output_dir.join("new-output.txt");
        let outside = tempfile::tempdir().unwrap();
        let moved_dir = outside.path().join("moved-generated");
        let redirect_dir = outside.path().join("redirect-generated");
        fs::create_dir(&redirect_dir).unwrap();
        let outputs = BTreeMap::from([(
            "generated/new-output.txt".to_owned(),
            "generated\n".to_owned(),
        )]);
        let destinations =
            BTreeMap::from([("generated/new-output.txt".to_owned(), destination.clone())]);
        let mut staged = stage_outputs(root.path(), outputs, destinations).unwrap();
        staged[0]
            .temporary
            .take()
            .unwrap()
            .persist_noclobber(&destination)
            .unwrap();

        let error = restore_output_with_hook(root.path(), &mut staged[0], || {
            fs::rename(&output_dir, &moved_dir).unwrap();
            fs::remove_file(moved_dir.join("new-output.txt")).unwrap();
            symlink(&redirect_dir, &output_dir).unwrap();
        })
        .unwrap_err();

        assert!(error.to_lowercase().contains("symlink"), "{error}");
        assert_eq!(fs::read_dir(&redirect_dir).unwrap().count(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn retained_backup_preserves_windows_readonly_attribute() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("output.txt");
        fs::write(&destination, b"original\n").unwrap();
        let mut permissions = fs::metadata(&destination).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&destination, permissions).unwrap();
        let outputs = BTreeMap::from([("output.txt".to_owned(), "generated\n".to_owned())]);
        let destinations = BTreeMap::from([("output.txt".to_owned(), destination.clone())]);
        let staged = stage_outputs(root.path(), outputs, destinations).unwrap();

        fs::remove_file(&destination).unwrap();
        fs::write(&destination, b"concurrent\n").unwrap();
        let error = commit_outputs(root.path(), staged, |_, _| Ok(())).unwrap_err();

        assert!(error.contains("changed"), "{error}");
        assert_eq!(fs::read(&destination).unwrap(), b"concurrent\n");
        let retained = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| fs::read(path).is_ok_and(|bytes| bytes == b"original\n"))
            .expect("original bytes were not retained in a recovery backup");
        assert!(fs::metadata(retained).unwrap().permissions().readonly());
    }

    #[cfg(windows)]
    #[test]
    fn rollback_restoration_preserves_windows_readonly_attribute() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("output.txt");
        fs::write(&destination, b"original\n").unwrap();
        let mut permissions = fs::metadata(&destination).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&destination, permissions).unwrap();
        let outputs = BTreeMap::from([("output.txt".to_owned(), "generated\n".to_owned())]);
        let destinations = BTreeMap::from([("output.txt".to_owned(), destination.clone())]);
        let mut staged = stage_outputs(root.path(), outputs, destinations).unwrap();

        let mut writable = fs::metadata(&destination).unwrap().permissions();
        writable.set_readonly(false);
        fs::set_permissions(&destination, writable).unwrap();
        staged[0]
            .temporary
            .take()
            .unwrap()
            .persist(&destination)
            .unwrap();
        restore_output(root.path(), &mut staged[0]).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"original\n");
        assert!(fs::metadata(destination).unwrap().permissions().readonly());
    }

    #[cfg(windows)]
    #[test]
    fn later_persist_error_rolls_back_prior_replacement() {
        use std::os::windows::fs::OpenOptionsExt;

        let root = tempfile::tempdir().unwrap();
        let existing_a = root.path().join("a-existing.txt");
        let existing_b = root.path().join("b-existing.txt");
        fs::write(&existing_a, b"old a\n").unwrap();
        fs::write(&existing_b, b"old b\n").unwrap();
        let outputs = BTreeMap::from([
            ("a-existing.txt".to_owned(), "new a\n".to_owned()),
            ("b-existing.txt".to_owned(), "new b\n".to_owned()),
        ]);
        let destinations = outputs
            .keys()
            .map(|relative| (relative.clone(), root.path().join(relative)))
            .collect();
        let staged = stage_outputs(root.path(), outputs, destinations).unwrap();
        let _locked = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0x0000_0003)
            .open(&existing_b)
            .unwrap();

        let error = commit_outputs(root.path(), staged, |_, _| Ok(())).unwrap_err();

        assert!(
            error.contains("Cannot persist generated output b-existing.txt"),
            "{error}"
        );
        assert_eq!(fs::read(&existing_a).unwrap(), b"old a\n");
        assert_eq!(fs::read(&existing_b).unwrap(), b"old b\n");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }
}
