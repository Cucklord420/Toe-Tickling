// SPDX-License-Identifier: AGPL-3.0-only
//! Independent read-only source-byte, digest, declaration and entry-point audit.
//! This crate does not import the guide renderer or establish provider behavior.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

type Result<T> = std::result::Result<T, String>;
const CANONICAL: &str = "https://github.com/Cucklord420/Toe-Tickling";
const SOURCES: &[&str] = &[
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
const FAMILIES: &[(&str, usize)] = &[
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
];
const REGISTER_FAMILIES: &[(&str, usize)] = &[
    ("AUTH", 6),
    ("EVID", 6),
    ("EXEC", 8),
    ("SAFE", 6),
    ("ENG", 7),
    ("DOM", 5),
    ("AUD", 8),
];
const ADAPTERS: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    ".github/copilot-instructions.md",
    ".cursor/rules/justin-guidebook.mdc",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema_version: u32,
    policy_version: String,
    updated: String,
    owner: String,
    repository: String,
    default_branch: String,
    canonical_url: String,
    entry_file: String,
    sources: Vec<String>,
    implementation_language: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Lock {
    policy_version: String,
    source_repo: String,
    source_sha256: String,
    guidebook_sha256: String,
    rule_count: usize,
    verification_rule_count: usize,
    sources: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Register {
    schema_version: u32,
    rules: Vec<RegisterRule>,
}
// Remaining rule semantics, dates and evidence belong to the primary validator.
#[derive(Debug, Deserialize)]
struct RegisterRule {
    id: String,
    status: String,
}
#[derive(Debug, PartialEq, Eq)]
struct Summary {
    sources: usize,
    rules: usize,
    verification_rules: usize,
}

fn no_symlink_ancestors(path: &Path) -> Result<()> {
    // Walk lexical prefixes: ancestors() can omit `alias` for `alias/.`, while
    // symlink_metadata("alias/.") follows that alias and hides the symlink.
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component);
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let metadata = fs::symlink_metadata(&prefix)
            .map_err(|e| format!("cannot inspect audit root ancestry: {e}"))?;
        if metadata.file_type().is_symlink() {
            return Err("symlink in audit root ancestry".into());
        }
    }
    Ok(())
}

fn safe(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty() || relative.contains('\\') {
        return Err(format!("unsafe path: {relative}"));
    }
    let mut path = root.to_path_buf();
    for part in Path::new(relative).components() {
        let Component::Normal(name) = part else {
            return Err(format!("unsafe path: {relative}"));
        };
        path.push(name);
        let metadata =
            fs::symlink_metadata(&path).map_err(|e| format!("cannot inspect {relative}: {e}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("symlink rejected: {relative}"));
        }
    }
    Ok(path)
}

fn read(root: &Path, relative: &str) -> Result<String> {
    fs::read_to_string(safe(root, relative)?).map_err(|e| format!("cannot read {relative}: {e}"))
}
fn json<T: for<'a> Deserialize<'a>>(root: &Path, relative: &str) -> Result<T> {
    serde_json::from_str(&read(root, relative)?).map_err(|e| format!("invalid {relative}: {e}"))
}

#[derive(Debug)]
struct Module {
    path: String,
    at: usize,
}
fn modules(guide: &str) -> Result<Vec<Module>> {
    let mut result = Vec::new();
    let mut offset = 0;
    for line in guide.split_inclusive('\n') {
        if let Some(raw) = line.strip_prefix("<!-- Source: ") {
            let path = raw
                .trim_end_matches('\n')
                .strip_suffix(" -->")
                .ok_or("malformed source marker")?;
            if !SOURCES.contains(&path) {
                return Err(format!("unsafe or unknown source marker: {path}"));
            }
            result.push(Module {
                path: path.to_owned(),
                at: offset,
            });
        }
        offset += line.len();
    }
    if result.is_empty() {
        return Err("no source markers found".into());
    }
    Ok(result)
}

fn valid_id(token: &str, digits: usize) -> bool {
    token.split_once('-').is_some_and(|(family, number)| {
        !family.is_empty()
            && family.bytes().all(|c| c.is_ascii_uppercase())
            && number.len() == digits
            && number.bytes().all(|c| c.is_ascii_digit())
    })
}

// Count declarations, never a bold ID quoted in explanatory prose or a fence.
fn ids(text: &str, digits: usize) -> Result<BTreeSet<String>> {
    let mut found = BTreeSet::new();
    let mut fence: Option<(char, usize)> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let first = trimmed.chars().next().unwrap_or(' ');
        if matches!(first, '`' | '~') {
            let length = trimmed.chars().take_while(|c| *c == first).count();
            if let Some((kind, minimum)) = fence {
                if kind == first && length >= minimum && trimmed[length..].trim().is_empty() {
                    fence = None;
                }
            } else if length >= 3 {
                fence = Some((first, length));
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let token = if digits == 3 {
            line.strip_prefix("- **")
                .and_then(|tail| tail.split_once("**"))
                .map(|(id, _)| id)
        } else {
            line.strip_prefix("### ")
                .and_then(|tail| tail.split_once(" — "))
                .map(|(id, _)| id)
        };
        if let Some(token) = token.filter(|token| valid_id(token, digits))
            && !found.insert(token.to_owned())
        {
            return Err(format!("duplicate rule ID: {token}"));
        }
    }
    if found.is_empty() {
        return Err("no rule declarations found".into());
    }
    Ok(found)
}

fn require_stable(ids: &BTreeSet<String>, families: &[(&str, usize)], digits: usize) -> Result<()> {
    for (family, count) in families {
        for number in 1..=*count {
            let id = format!("{family}-{number:0digits$}");
            if !ids.contains(&id) {
                return Err(format!("missing stable rule: {id}"));
            }
        }
    }
    Ok(())
}

fn adapters(root: &Path) -> Result<()> {
    for entry in ADAPTERS {
        let lower = read(root, entry)?.to_ascii_lowercase();
        let full_guide = lower.lines().any(|line| {
            line.contains("guidebook.md")
                && (line.contains("read the complete") || line.contains("in full"))
        });
        let audits = lower.contains("aud-001") && lower.contains("aud-020");
        let file_loading = lower.contains("file tools") || lower.contains("read this file");
        if !full_guide || !audits || !file_loading || !lower.contains("rust") {
            return Err(format!(
                "missing full-guide/audit/Rust/actual-file instruction: {entry}"
            ));
        }
    }
    Ok(())
}

fn audit(root: &Path) -> Result<Summary> {
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        env::current_dir().map_err(|e| e.to_string())?.join(root)
    };
    no_symlink_ancestors(&absolute)?;
    let root = fs::canonicalize(absolute).map_err(|e| format!("invalid audit root: {e}"))?;
    let policy: Policy = json(&root, "policy.json")?;
    let lock: Lock = json(&root, "guidebook-lock.json")?;
    let source_names: Vec<String> = SOURCES.iter().map(|s| (*s).to_owned()).collect();
    if policy.schema_version != 1
        || policy.owner != "Cucklord420"
        || policy.repository != "Toe-Tickling"
        || policy.canonical_url != CANONICAL
        || policy.entry_file != "GUIDEBOOK.md"
        || policy.default_branch.trim().is_empty()
        || policy.implementation_language != "Rust"
        || lock.source_repo != CANONICAL
        || policy.policy_version != lock.policy_version
    {
        return Err("policy identity/version/Rust requirement differs".into());
    }
    if policy.sources != source_names || lock.sources != source_names {
        return Err("policy/lock source order or coverage differs".into());
    }
    let guide = read(&root, "GUIDEBOOK.md")?;
    let header = guide
        .split_once("<!-- Source: ")
        .map(|(header, _)| header)
        .ok_or("guidebook header or modules missing")?;
    for line in [
        "# Justin's complete AI guidebook".to_owned(),
        format!(
            "Version: {} | Updated: {}",
            policy.policy_version, policy.updated
        ),
        format!("Canonical repository: {CANONICAL}"),
        format!("Source SHA-256: `{}`", lock.source_sha256),
    ] {
        if !header.lines().any(|actual| actual == line) {
            return Err("guidebook header differs from policy/lock".into());
        }
    }
    let parts = modules(&guide)?;
    if parts
        .iter()
        .map(|part| part.path.as_str())
        .collect::<Vec<_>>()
        != SOURCES
    {
        return Err("guide source marker order or coverage differs".into());
    }
    let mut source_digest = Sha256::new();
    let mut declared = BTreeSet::new();
    for (index, item) in parts.iter().enumerate() {
        let content = read(&root, &item.path)?;
        source_digest.update(item.path.as_bytes());
        source_digest.update([0]);
        source_digest.update(content.as_bytes());
        source_digest.update([0]);
        let rendered = format!("<!-- Source: {} -->\n\n{}\n", item.path, content.trim_end());
        let expected = if index + 1 == parts.len() {
            rendered
        } else {
            format!("{rendered}\n---\n\n")
        };
        let end = parts.get(index + 1).map_or(guide.len(), |part| part.at);
        if guide.get(item.at..end).ok_or("invalid marker offset")? != expected {
            return Err(format!(
                "generated content differs from source: {}",
                item.path
            ));
        }
        if item.path.starts_with("rules/") || item.path.starts_with("profile/") {
            for id in ids(&content, 3)? {
                if !declared.insert(id.clone()) {
                    return Err(format!("duplicate rule ID: {id}"));
                }
            }
        }
    }
    if format!("{:x}", source_digest.finalize()) != lock.source_sha256
        || format!("{:x}", Sha256::digest(guide.as_bytes())) != lock.guidebook_sha256
    {
        return Err("source or guidebook digest differs from lock".into());
    }
    require_stable(&declared, FAMILIES, 3)?;
    if ids(&guide, 3)? != declared || lock.rule_count != declared.len() {
        return Err("lock/bundle module rule count or declaration coverage differs".into());
    }
    let register: Register = json(&root, "policy/rules.json")?;
    if register.schema_version != 1 {
        return Err("unsupported verification register schema".into());
    }
    let mut registered = BTreeSet::new();
    let mut active = 0;
    for rule in register.rules {
        if !valid_id(&rule.id, 2) || !registered.insert(rule.id) {
            return Err("invalid or duplicate verification rule ID".into());
        }
        match rule.status.as_str() {
            "active" => active += 1,
            "superseded" => {}
            _ => return Err("invalid verification rule status".into()),
        }
    }
    require_stable(&registered, REGISTER_FAMILIES, 2)?;
    if ids(&read(&root, "RULEBOOK.md")?, 2)? != registered
        || ids(&guide, 2)? != registered
        || lock.verification_rule_count != active
    {
        return Err("verification register/bundle/lock coverage differs".into());
    }
    adapters(&root)?;
    Ok(Summary {
        sources: parts.len(),
        rules: declared.len(),
        verification_rules: active,
    })
}

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.first().map(String::as_str) != Some("check") || arguments.len() > 2 {
        eprintln!("usage: guidebook-audit check [repository-root]");
        return ExitCode::FAILURE;
    }
    let root = if arguments.len() == 2 {
        PathBuf::from(&arguments[1])
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    };
    match audit(&root) {
        Ok(summary) => {
            println!(
                "PASS: {} modules; {} stable module rules; {} active verification rules; provider behavior NOT tested",
                summary.sources, summary.rules, summary.verification_rules
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("FAIL: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use tempfile::TempDir;

    struct Fixture {
        _work: TempDir,
        root: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let work = tempfile::Builder::new()
                .prefix("independent Rust audit ")
                .tempdir()
                .unwrap();
            let root = fs::canonicalize(work.path())
                .unwrap()
                .join("guide with spaces");
            fs::create_dir(&root).unwrap();
            let fixture = Self { _work: work, root };
            for (index, (family, count)) in FAMILIES.iter().enumerate() {
                fixture.write(
                    SOURCES[index],
                    &(1..=*count)
                        .map(|n| format!("- **{family}-{n:03}** Synthetic requirement.\n"))
                        .collect::<String>(),
                );
            }
            let mut all_rules = Vec::new();
            let mut rulebook = String::from("# Synthetic verification register\n\n");
            for (family, count) in REGISTER_FAMILIES {
                for n in 1..=*count {
                    let id = format!("{family}-{n:02}");
                    rulebook.push_str(&format!("### {id} — Synthetic requirement\n\n"));
                    all_rules.push(json!({"id": id, "status": "active"}));
                }
            }
            fixture.write("RULEBOOK.md", &rulebook);
            fixture.write("docs/SELF_CHECKS.md", "# Synthetic self checks\n");
            fixture.write("docs/EVOLUTION.md", "# Synthetic evolution\n");
            fixture.write(
                "policy/rules.json",
                &json!({"schema_version": 1, "rules": all_rules}).to_string(),
            );
            fixture.write(
                "policy.json",
                &json!({"schema_version": 1, "policy_version": "1.2.0",
                "updated": "10-15-2026", "owner": "Cucklord420", "repository": "Toe-Tickling",
                "default_branch": "main", "canonical_url": CANONICAL, "entry_file": "GUIDEBOOK.md",
                "sources": SOURCES, "implementation_language": "Rust"})
                .to_string(),
            );
            for entry in ADAPTERS {
                fixture.write(entry, "Read the complete GUIDEBOOK.md with file tools.\nAudit the instructions themselves under AUD-001 through AUD-020. All code MUST be Rust.\n");
            }
            fixture.rebuild();
            fixture
        }
        fn write(&self, relative: &str, text: &str) {
            let path = self.root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        fn read(&self, relative: &str) -> String {
            fs::read_to_string(self.root.join(relative)).unwrap()
        }
        // Fixtures never import the primary renderer or its library.
        fn rebuild(&self) {
            let mut digest = Sha256::new();
            let mut body = String::new();
            let mut count = 0;
            for (index, source) in SOURCES.iter().enumerate() {
                let content = self.read(source);
                digest.update(source.as_bytes());
                digest.update([0]);
                digest.update(content.as_bytes());
                digest.update([0]);
                if index > 0 {
                    body.push_str("\n---\n\n");
                }
                body.push_str(&format!(
                    "<!-- Source: {source} -->\n\n{}\n",
                    content.trim_end()
                ));
                if source.starts_with("rules/") || source.starts_with("profile/") {
                    count += ids(&content, 3).unwrap().len();
                }
            }
            let hash = format!("{:x}", digest.finalize());
            let guide = format!(
                "# Justin's complete AI guidebook\n\nVersion: 1.2.0 | Updated: 10-15-2026\n\nCanonical repository: {CANONICAL}\n\nSource SHA-256: `{hash}`\n\nSynthetic header.\n\n---\n\n{body}"
            );
            self.write("GUIDEBOOK.md", &guide);
            self.write("guidebook-lock.json", &json!({"policy_version": "1.2.0", "source_repo": CANONICAL,
                "source_sha256": hash, "guidebook_sha256": format!("{:x}", Sha256::digest(guide.as_bytes())),
                "rule_count": count, "verification_rule_count": 46, "sources": SOURCES}).to_string());
        }
        fn alter_lock(&self, field: &str, value: Value) {
            let mut lock: Value = serde_json::from_str(&self.read("guidebook-lock.json")).unwrap();
            lock[field] = value;
            self.write("guidebook-lock.json", &lock.to_string());
        }
    }

    #[test]
    fn full_synthetic_bundle_passes_with_separate_counts() {
        let f = Fixture::new();
        assert_eq!(
            audit(&f.root).unwrap(),
            Summary {
                sources: 14,
                rules: 159,
                verification_rules: 46
            }
        );
    }
    #[test]
    fn bold_prose_and_fenced_examples_do_not_declare_rules() {
        assert_eq!(ids("- **AUD-001** actual\n**AUD-001** prose\n```md\n- **AUD-002** example\n```\n~~~md\n- **AUD-003** example\n~~~\n", 3).unwrap(), BTreeSet::from(["AUD-001".to_owned()]));
        assert!(ids("ordinary **AUD-001** prose", 3).is_err());
        assert!(ids("- **ENG-0000** malformed", 3).is_err());
    }
    #[test]
    fn duplicate_declarations_and_missing_stable_rules_fail() {
        assert!(
            ids("- **AUD-001** one\n- **AUD-001** two\n", 3)
                .unwrap_err()
                .contains("duplicate")
        );
        let f = Fixture::new();
        for (source, missing) in [
            (SOURCES[4], "ENG-019"),
            (SOURCES[4], "ENG-020"),
            (SOURCES[7], "AUD-020"),
            (SOURCES[8], "EVO-028"),
        ] {
            let original = f.read(source);
            f.write(source, &original.replace(missing, "ALT-999"));
            f.rebuild();
            assert!(audit(&f.root).unwrap_err().contains(missing));
            f.write(source, &original);
        }
    }
    #[test]
    fn source_bytes_and_trimmed_trailing_changes_are_detected() {
        let f = Fixture::new();
        let original = f.read(SOURCES[0]);
        f.write(SOURCES[0], &(original.clone() + "new source text\n"));
        assert!(audit(&f.root).unwrap_err().contains("differs from source"));
        f.write(SOURCES[0], &(original + " \n"));
        assert!(audit(&f.root).unwrap_err().contains("digest"));
    }
    #[test]
    fn guide_tampering_preamble_and_extra_tail_are_detected() {
        let f = Fixture::new();
        let original = f.read("GUIDEBOOK.md");
        for changed in [
            original.clone() + "extra tail\n",
            "Injected preamble\n".to_owned() + &original,
            original.replace("Synthetic header.", "Tampered header."),
        ] {
            f.write("GUIDEBOOK.md", &changed);
            assert!(audit(&f.root).is_err());
        }
    }
    #[test]
    fn markers_require_exact_order_coverage_and_safe_paths() {
        let f = Fixture::new();
        let guide = f.read("GUIDEBOOK.md");
        for marker in [
            "<!-- Source: docs/EVOLUTION.md -->",
            "",
            "<!-- Source: private/secret.md -->",
            "<!-- Source: rules/../RULEBOOK.md -->",
        ] {
            f.write(
                "GUIDEBOOK.md",
                &guide.replace("<!-- Source: RULEBOOK.md -->", marker),
            );
            assert!(audit(&f.root).is_err());
        }
        for marker in [
            "/etc/passwd.md",
            "private/secret.md",
            "docs/unknown.md",
            "rules/a\\b.md",
        ] {
            assert!(modules(&format!("<!-- Source: {marker} -->\n")).is_err());
        }
        assert!(modules("<!-- Source: RULEBOOK.md --> trailing\n").is_err());
        assert!(modules("no markers").is_err());
    }
    #[test]
    fn compact_json_works_and_duplicate_or_invalid_lock_fields_fail() {
        let f = Fixture::new();
        let original = f.read("guidebook-lock.json");
        assert!(audit(&f.root).is_ok());
        f.write(
            "guidebook-lock.json",
            &original.replacen('{', "{\"rule_count\":158,", 1),
        );
        assert!(audit(&f.root).unwrap_err().contains("duplicate field"));
        for invalid in [json!(-1), json!(158.5), json!("158"), json!(null)] {
            f.write("guidebook-lock.json", &original);
            f.alter_lock("rule_count", invalid);
            assert!(
                audit(&f.root)
                    .unwrap_err()
                    .contains("invalid guidebook-lock.json")
            );
        }
        f.write("guidebook-lock.json", &(original + " trailing"));
        assert!(audit(&f.root).is_err());
    }
    #[test]
    fn lock_counts_and_order_must_match_both_rule_sets() {
        let f = Fixture::new();
        let original = f.read("guidebook-lock.json");
        for (field, value) in [
            ("rule_count", json!(203)),
            ("verification_rule_count", json!(44)),
            ("sources", json!(SOURCES.iter().rev().collect::<Vec<_>>())),
        ] {
            f.write("guidebook-lock.json", &original);
            f.alter_lock(field, value);
            assert!(audit(&f.root).is_err());
        }
    }
    #[test]
    fn removed_rust_verification_rule_fails_even_with_consistent_artifacts() {
        let f = Fixture::new();
        let mut register: Value = serde_json::from_str(&f.read("policy/rules.json")).unwrap();
        register["rules"]
            .as_array_mut()
            .unwrap()
            .retain(|rule| rule["id"] != "ENG-07");
        f.write("policy/rules.json", &register.to_string());
        f.write(
            "RULEBOOK.md",
            &f.read("RULEBOOK.md")
                .replace("### ENG-07 — Synthetic requirement\n\n", ""),
        );
        f.rebuild();
        f.alter_lock("verification_rule_count", json!(45));
        assert!(audit(&f.root).unwrap_err().contains("ENG-07"));
    }

    #[test]
    fn valid_retained_history_supersession_passes_independent_audit() {
        let f = Fixture::new();
        let mut register: Value = serde_json::from_str(&f.read("policy/rules.json")).unwrap();
        let rules = register["rules"].as_array_mut().unwrap();
        let old = rules
            .iter_mut()
            .find(|rule| rule["id"] == "ENG-07")
            .unwrap();
        old["status"] = json!("superseded");
        rules.push(json!({
            "id": "ENG-47",
            "status": "active",
            "supersedes": ["ENG-07"]
        }));
        f.write("policy/rules.json", &register.to_string());

        let mut rulebook = f.read("RULEBOOK.md");
        rulebook.push_str("### ENG-47 — Synthetic successor\n\n");
        f.write("RULEBOOK.md", &rulebook);
        f.rebuild();

        assert_eq!(
            audit(&f.root).unwrap(),
            Summary {
                sources: 14,
                rules: 159,
                verification_rules: 46
            }
        );
    }

    #[test]
    fn register_identity_and_status_must_match_rendered_headings() {
        let f = Fixture::new();
        let original = f.read("policy/rules.json");
        let mut register: Value = serde_json::from_str(&original).unwrap();
        register["rules"][0]["id"] = json!("AUTH-99");
        f.write("policy/rules.json", &register.to_string());
        assert!(audit(&f.root).is_err());
        let mut register: Value = serde_json::from_str(&original).unwrap();
        register["rules"][0]["status"] = json!("superseded");
        f.write("policy/rules.json", &register.to_string());
        assert!(audit(&f.root).unwrap_err().contains("verification"));
    }
    #[test]
    fn adapters_require_actual_full_file_loading_audits_and_rust() {
        let f = Fixture::new();
        let original = f.read("CLAUDE.md");
        for absent in ["Read the complete", "file tools", "AUD-020", "Rust"] {
            f.write("CLAUDE.md", &original.replace(absent, "removed"));
            assert!(
                audit(&f.root)
                    .unwrap_err()
                    .contains("instruction: CLAUDE.md")
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn symlink_root_ancestor_source_and_adapter_are_rejected() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let alias = f._work.path().join("alias");
        symlink(&f.root, &alias).unwrap();
        assert!(audit(&alias).unwrap_err().contains("symlink"));
        assert!(audit(&alias.join(".")).unwrap_err().contains("symlink"));
        for relative in [SOURCES[0], "CLAUDE.md"] {
            let path = f.root.join(relative);
            let backup = path.with_extension("backup.md");
            fs::rename(&path, &backup).unwrap();
            symlink(&backup, &path).unwrap();
            assert!(audit(&f.root).unwrap_err().contains("symlink"));
            fs::remove_file(&path).unwrap();
            fs::rename(backup, path).unwrap();
        }
    }
}
