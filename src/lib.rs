//! Mechanical validation only: these checks do not establish truthful model behavior.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path},
};
use time::{Date, OffsetDateTime, format_description::well_known::Rfc3339};

pub type Result<T> = std::result::Result<T, String>;
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
    for path in READING_FILES {
        documents.insert((*path).to_owned(), sha256(&root.join(path))?);
    }
    Ok(Manifest {
        schema_version: 1,
        documents,
    })
}
fn files(root: &Path, here: &Path, output: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for entry in fs::read_dir(here).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if here == root
            && [".git", "target", "private", "local-data"]
                .iter()
                .any(|x| entry.file_name() == *x)
        {
            continue;
        }
        require(
            !entry.file_type().map_err(|e| e.to_string())?.is_symlink(),
            format!("Symlink outside audit scope: {}", path.display()),
        )?;
        if path.is_dir() {
            files(root, &path, output)?;
        } else {
            output.push(path);
        }
    }
    Ok(())
}
pub fn validate_links(root: &Path) -> Result<()> {
    let mut paths = Vec::new();
    files(root, root, &mut paths)?;
    for path in paths {
        let extension = path.extension().and_then(|x| x.to_str()).unwrap_or("");
        require(
            ["rs", "md", "json", "toml", "lock", "yml", "yaml", "txt"].contains(&extension)
                || ["LICENSE", ".gitignore"]
                    .iter()
                    .any(|name| path.file_name().is_some_and(|file| file == *name)),
            format!("Executable source must be Rust: {}", path.display()),
        )?;
        if extension != "md" {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        for tail in text.split("](").skip(1) {
            let target = tail
                .split(')')
                .next()
                .unwrap_or("")
                .trim_matches(['<', '>']);
            if target.contains("://") || target.starts_with("mailto:") || target.starts_with('#') {
                continue;
            }
            let target = target.split('#').next().unwrap_or("");
            require(
                !target.is_empty()
                    && !Path::new(target).is_absolute()
                    && !Path::new(target)
                        .components()
                        .any(|c| matches!(c, Component::ParentDir)),
                format!("Unsafe local link in {}: {target}", path.display()),
            )?;
            require(
                path.parent().unwrap_or(root).join(target).exists(),
                format!("Broken link in {}: {target}", path.display()),
            )?;
        }
    }
    Ok(())
}
pub fn check_repo(root: &Path, today: Date) -> Result<Policy> {
    let policy = load_policy(root)?;
    validate_policy(&policy, today)?;
    let actual = fs::read_to_string(root.join("RULEBOOK.md")).map_err(|e| e.to_string())?;
    require(
        actual == render(&policy),
        "RULEBOOK.md differs from canonical rules; run render and review",
    )?;
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
pub fn validate_audit(root: &Path, policy: &Policy, a: &Audit, now: OffsetDateTime) -> Result<()> {
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
    let head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|e| e.to_string())?;
    require(
        head.status.success()
            && String::from_utf8_lossy(&head.stdout).trim() == a.repository_revision,
        "Audit revision does not match checkout HEAD",
    )?;
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
    let ids: BTreeSet<_> = policy
        .rules
        .iter()
        .filter(|r| r.status == "active")
        .map(|r| r.id.clone())
        .collect();
    let mut checked = BTreeSet::new();
    for c in &a.checks {
        require(
            checked.insert(c.id.clone()) && ids.contains(&c.id),
            format!("Duplicate or unknown rule check {}", c.id),
        )?;
        check_disposition(c, &a.evidence, false)?;
        require(
            c.status != "not-applicable"
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
    Ok(())
}
pub fn new_audit(root: &Path, task: String) -> Result<Audit> {
    let policy = load_policy(root)?;
    let head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|e| e.to_string())?;
    require(head.status.success(), "Cannot identify repository HEAD")?;
    Ok(Audit {
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
            .collect(),
        acceptance: vec![Check {
            id: "AC-01".into(),
            status: "blocked".into(),
            reason: "UNFILLED: replace with actual task acceptance conditions".into(),
            evidence: vec![],
        }],
    })
}
