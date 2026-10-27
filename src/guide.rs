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
struct StagedOutput {
    relative: String,
    destination: PathBuf,
    temporary: Option<tempfile::NamedTempFile>,
}

fn stage_outputs(
    outputs: BTreeMap<String, String>,
    destinations: BTreeMap<String, PathBuf>,
) -> Result<Vec<StagedOutput>> {
    let mut staged = Vec::with_capacity(outputs.len());
    for (relative, text) in outputs {
        let destination = destinations
            .get(&relative)
            .ok_or_else(|| format!("Missing preflight output path: {relative}"))?;
        let parent = destination.parent().ok_or("No output parent")?;
        fs::create_dir_all(parent).map_err(|e| format!("{relative}: {e}"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|e| format!("Cannot stage {relative}: {e}"))?;
        temporary
            .write_all(text.as_bytes())
            .map_err(|e| format!("Cannot stage {relative}: {e}"))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|e| format!("Cannot stage {relative}: {e}"))?;
        staged.push(StagedOutput {
            relative,
            destination: destination.clone(),
            temporary: Some(temporary),
        });
    }
    Ok(staged)
}

fn commit_outputs<F>(staged: Vec<StagedOutput>, mut after_persist: F) -> Result<usize>
where
    F: FnMut(&str, usize) -> Result<()>,
{
    let count = staged.len();
    let mut committed = 0;
    for mut output in staged {
        let temporary = output
            .temporary
            .take()
            .ok_or_else(|| format!("Missing staged file: {}", output.relative))?;
        temporary
            .persist(&output.destination)
            .map_err(|e| format!("Cannot persist generated output {}: {e}", output.relative))?;
        committed += 1;
        after_persist(&output.relative, committed)?;
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
    let staged = stage_outputs(outputs, destinations)?;
    commit_outputs(staged, |_, _| Ok(()))
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
        let staged = stage_outputs(outputs, destinations).unwrap();

        let result = commit_outputs(staged, |_, committed| {
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
        let staged = stage_outputs(outputs, destinations).unwrap();

        let result = commit_outputs(staged, |_, _| {
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
        let staged = stage_outputs(outputs, destinations).unwrap();
        fs::write(&destination, b"concurrent\n").unwrap();

        let error = commit_outputs(staged, |_, _| Ok(())).unwrap_err();

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
}
