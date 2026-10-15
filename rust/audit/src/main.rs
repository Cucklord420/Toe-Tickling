// SPDX-License-Identifier: AGPL-3.0-only
//! Read-only Rust policy-integrity audit; Python remains the reference implementation.
use std::{collections::BTreeSet, env, fs, path::{Component, Path, PathBuf}, process::ExitCode};

fn safe(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let mut path = root.to_path_buf();
    for part in Path::new(relative).components() {
        let Component::Normal(name) = part else { return Err(format!("unsafe path: {relative}")); };
        path.push(name);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|e| format!("cannot inspect {relative}: {e}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("symlink rejected: {relative}"));
        }
    }
    Ok(path)
}

#[derive(Debug)]
struct Module { path: String, at: usize }

fn modules(guide: &str) -> Result<Vec<Module>, String> {
    let mut result = Vec::new();
    let mut offset = 0;
    for line in guide.split_inclusive('\n') {
        if let Some(raw) = line.strip_prefix("<!-- Source: ") {
            let path = raw.trim_end_matches('\n').strip_suffix(" -->")
                .ok_or("malformed source marker")?;
            let components = Path::new(path);
            if components.components().any(|c| !matches!(c, Component::Normal(_)))
                || !(path.starts_with("rules/") || path.starts_with("profile/"))
                || !path.ends_with(".md") {
                return Err(format!("unsafe source marker: {path}"));
            }
            result.push(Module { path: path.to_owned(), at: offset });
        }
        offset += line.len();
    }
    if result.is_empty() { return Err("no source markers found".into()); }
    Ok(result)
}

fn ids(text: &str) -> Result<BTreeSet<String>, String> {
    let mut found = BTreeSet::new();
    for (index, token) in text.split("**").enumerate() {
        if index % 2 == 0 { continue; }
        let Some((family, number)) = token.split_once('-') else { continue; };
        if family.is_empty() || !family.bytes().all(|c| c.is_ascii_uppercase())
            || number.len() != 3 || !number.bytes().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if !found.insert(token.to_owned()) {
            return Err(format!("duplicate rule ID: {token}"));
        }
    }
    if found.is_empty() { return Err("no rule IDs found".into()); }
    for n in 1..=20 {
        let id = format!("AUD-{n:03}");
        if !found.contains(&id) { return Err(format!("missing audit rule: {id}")); }
    }
    for n in 1..=28 {
        let id = format!("EVO-{n:03}");
        if !found.contains(&id) { return Err(format!("missing evolution rule: {id}")); }
    }
    Ok(found)
}

fn audit(root: &Path) -> Result<(usize, usize), String> {
    let guide = fs::read_to_string(safe(root, "GUIDEBOOK.md")?)
        .map_err(|e| format!("cannot read guidebook: {e}"))?;
    for part in ["Version:", "Canonical repository:", "Source SHA-256:"] {
        if !guide.contains(part) { return Err(format!("missing guidebook header: {part}")); }
    }
    let parts = modules(&guide)?;
    let mut seen = BTreeSet::new();
    for (index, item) in parts.iter().enumerate() {
        if !seen.insert(item.path.as_str()) {
            return Err(format!("duplicate source marker: {}", item.path));
        }
        let content = fs::read_to_string(safe(root, &item.path)?)
            .map_err(|e| format!("cannot read {}: {e}", item.path))?;
        let rendered = format!("<!-- Source: {} -->\n\n{}\n",
            item.path, content.trim_end());
        let expected = if index + 1 == parts.len() {
            rendered
        } else { format!("{rendered}\n---\n\n") };
        let end = parts.get(index + 1).map_or(guide.len(), |p| p.at);
        let actual = guide.get(item.at..end).ok_or("invalid marker offset")?;
        if actual != expected {
            return Err(format!("generated content differs from source: {}", item.path));
        }
    }
    for required in ["rules/00-authority.md", "rules/05-execution-contract.md",
                     "rules/40-security.md", "rules/60-instruction-audits.md",
                     "rules/65-continuous-evolution.md", "profile/PUBLIC.md"] {
        if !seen.contains(required) {
            return Err(format!("required source absent: {required}"));
        }
    }
    let rules = ids(&guide)?;
    let lock = fs::read_to_string(safe(root, "guidebook-lock.json")?)
        .map_err(|e| format!("cannot read lock: {e}"))?;
    let count = lock.lines().find_map(|line| line.trim()
        .strip_prefix("\"rule_count\":")?
        .trim().trim_end_matches(',').parse::<usize>().ok())
        .ok_or("lock rule_count missing")?;
    if count != rules.len() {
        return Err(format!("lock rule count differs: {count} != {}", rules.len()));
    }
    for entry in ["AGENTS.md", "CLAUDE.md", "GEMINI.md",
                  ".github/copilot-instructions.md",
                  ".cursor/rules/justin-guidebook.mdc"] {
        let text = fs::read_to_string(safe(root, entry)?)
            .map_err(|e| format!("cannot read {entry}: {e}"))?;
        if !text.contains("GUIDEBOOK.md") || !text.contains("audit the instructions themselves") {
            return Err(format!("missing full-guide/audit instruction: {entry}"));
        }
    }
    Ok((seen.len(), rules.len()))
}

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.first().map(String::as_str) != Some("check") || arguments.len() > 2 {
        eprintln!("usage: guidebook-audit check [repository-root]");
        return ExitCode::FAILURE;
    }
    let root = if arguments.len() == 2 { PathBuf::from(&arguments[1]) }
        else { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") };
    match audit(&root) {
        Ok((sources, rules)) => {
            println!("PASS: {sources} modules; {rules} stable rules; provider behavior NOT tested");
            ExitCode::SUCCESS
        }
        Err(error) => { eprintln!("FAIL: {error}"); ExitCode::FAILURE }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> String {
        (1..=20).map(|i| format!("**AUD-{i:03}** audit\n")).collect()
    }
    #[test] fn all_audits_present() { assert_eq!(ids(&valid()).unwrap().len(), 20); }
    #[test] fn missing_audits_fail() { assert!(ids("**AUTH-001**").is_err()); }
    #[test] fn no_rules_fail() { assert!(ids("nothing here").is_err()); }
    #[test] fn duplicate_ids_fail() {
        assert!(ids(&(valid()+"**AUTH-001** a\n**AUTH-001** b\n"))
            .unwrap_err().contains("duplicate"));
    }
    #[test] fn ordinary_bold_ignored() {
        assert_eq!(ids(&(valid()+"**warning**")).unwrap().len(), 20);
    }
    #[test] fn extra_rules_work() {
        assert_eq!(ids(&(valid()+"**ENG-014**")).unwrap().len(), 21);
    }
    #[test] fn malformed_numeric_rule_ignored() {
        assert_eq!(ids(&(valid()+"**ENG-0000**")).unwrap().len(), 20);
    }
    #[test] fn valid_source_marker() {
        assert_eq!(modules("Header\n<!-- Source: rules/test.md -->\n\n").unwrap()[0].at,7);
    }
    #[test] fn no_source_markers_fail() { assert!(modules("text").is_err()); }
    #[test] fn traversal_marker_rejected() {
        assert!(modules("<!-- Source: rules/../secret.md -->\n").is_err());
    }
    #[test] fn absolute_marker_rejected() {
        assert!(modules("<!-- Source: /etc/passwd.md -->\n").is_err());
    }
    #[test] fn wrong_directory_rejected() {
        assert!(modules("<!-- Source: private/secret.md -->\n").is_err());
    }
    #[test] fn malformed_marker_rejected() {
        assert!(modules("<!-- Source: rules/test.md --> trailing\n").is_err());
    }
}
