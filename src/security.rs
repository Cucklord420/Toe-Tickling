//! Public-tree checks. Diagnostics identify the file, never the matched private value.
use pulldown_cmark::{Event, Parser, Tag};
use regex::Regex;
use serde_yaml_ng::{Mapping, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

use crate::Result;

fn fail(message: &str, relative: &Path) -> String {
    format!("{message}: {}", relative.display())
}

/// Reject symlinks before canonicalization can hide an ancestor link.
fn no_symlink_ancestors(path: &Path) -> Result<()> {
    // Inspect each lexical prefix before following a later `.` or `..`.
    // Walking ancestors backwards can omit the very symlink hidden by `alias/.`.
    // Native components retain Windows prefixes and root semantics.
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&prefix) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("Refusing a public path with a symlink ancestor".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Cannot inspect public path ancestry".into()),
        }
    }
    Ok(())
}

/// Resolve a root only after rejecting symlinks in its original lexical path.
pub fn checked_root(root: &Path) -> Result<PathBuf> {
    let absolute = if root.is_absolute() {
        root.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| "Cannot determine current directory")?
            .join(root)
    };
    no_symlink_ancestors(&absolute)?;
    fs::canonicalize(absolute).map_err(|_| "Cannot resolve public-tree root".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn checked_root_accepts_native_drive_prefix() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(checked_root(root).unwrap(), fs::canonicalize(root).unwrap());
    }
}

fn skipped(relative: &Path) -> bool {
    relative
        .components()
        .any(|part| part.as_os_str() == ".git" || part.as_os_str() == "target")
}

fn private_path(relative: &Path) -> bool {
    relative.components().any(|part| {
        matches!(
            part.as_os_str()
                .to_str()
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("private" | ".private" | ".local")
        )
    }) || relative
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let folded = name.to_ascii_lowercase();
            matches!(folded.as_str(), "private.local.md" | "claude.local.md")
                || (folded.starts_with(".env") && name != ".env.example")
        })
}

fn checked_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
        || relative.to_string_lossy().contains('\\')
    {
        return Err("Unsafe path returned by public-tree enumeration".into());
    }
    let path = root.join(relative);
    no_symlink_ancestors(&path)?;
    Ok(path)
}

fn git_command(root: &Path, args: &[&str]) -> Command {
    let mut command = crate::git_command(root);
    command.args(args);
    command
}

fn git_root(root: &Path) -> Result<bool> {
    let git_root = git_command(root, &["rev-parse", "--show-toplevel"]).output();
    match git_root {
        Ok(output) if output.status.success() => {
            let directory =
                String::from_utf8(output.stdout).map_err(|_| "Git root is not valid UTF-8")?;
            let directory = fs::canonicalize(directory.trim_end_matches(['\r', '\n']))
                .map_err(|_| "Cannot resolve Git root")?;
            if directory == root {
                Ok(true)
            } else if root.join(".git").exists() {
                Err("Git working-tree root does not match selected public tree".into())
            } else {
                Ok(false)
            }
        }
        _ if root.join(".git").exists() => Err("Cannot enumerate the Git public tree".into()),
        _ => Ok(false),
    }
}

/// Return stage-zero regular-file entries and their immutable object IDs.
/// Conflicted and non-regular index entries cannot be safely treated as a
/// single publishable file, so reject them before reading any content.
fn staged_entries(root: &Path) -> Result<Option<BTreeMap<PathBuf, String>>> {
    if !git_root(root)? {
        return Ok(None);
    }
    let output = git_command(root, &["ls-files", "--stage", "-z"])
        .output()
        .map_err(|_| "Cannot list staged Git blobs")?;
    if !output.status.success() {
        return Err("Cannot list staged Git blobs".into());
    }
    let mut entries = BTreeMap::new();
    for record in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|row| !row.is_empty())
    {
        let separator = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("Malformed staged Git entry")?;
        let header =
            std::str::from_utf8(&record[..separator]).map_err(|_| "Malformed staged Git entry")?;
        let path = std::str::from_utf8(&record[separator + 1..])
            .map_err(|_| "Public file name is not valid UTF-8")?;
        let mut fields = header.split_ascii_whitespace();
        let mode = fields.next().ok_or("Malformed staged Git entry")?;
        let oid = fields.next().ok_or("Malformed staged Git entry")?;
        let stage = fields.next().ok_or("Malformed staged Git entry")?;
        if fields.next().is_some() || stage != "0" {
            return Err(fail(
                "Conflicted Git index entry requires review",
                Path::new(path),
            ));
        }
        if !matches!(mode, "100644" | "100755") {
            return Err(fail(
                "Public staged path must be a regular file, not a symlink or special entry",
                Path::new(path),
            ));
        }
        if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Malformed staged Git object ID".into());
        }
        if entries
            .insert(PathBuf::from(path), oid.to_owned())
            .is_some()
        {
            return Err(fail("Duplicate staged Git path", Path::new(path)));
        }
    }
    Ok(Some(entries))
}

fn staged_deletions(root: &Path) -> Result<BTreeSet<PathBuf>> {
    if !git_root(root)? {
        return Ok(BTreeSet::new());
    }
    let output = git_command(
        root,
        &[
            "diff",
            "--cached",
            "--no-renames",
            "--diff-filter=D",
            "--name-only",
            "-z",
        ],
    )
    .output()
    .map_err(|_| "Cannot list staged Git deletions")?;
    if !output.status.success() {
        return Err("Cannot list staged Git deletions".into());
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| {
            std::str::from_utf8(name)
                .map(PathBuf::from)
                .map_err(|_| "Public file name is not valid UTF-8".into())
        })
        .collect()
}

fn staged_blob(root: &Path, oid: &str, relative: &Path) -> Result<Vec<u8>> {
    let output = git_command(root, &["cat-file", "blob", oid])
        .output()
        .map_err(|_| fail("Cannot read staged Git blob", relative))?;
    if !output.status.success() {
        return Err(fail("Cannot read staged Git blob", relative));
    }
    Ok(output.stdout)
}

fn walk(root: &Path, here: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(here).map_err(|_| "Cannot enumerate public tree")? {
        let entry = entry.map_err(|_| "Cannot inspect public-tree entry")?;
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| "Public-tree path leaves root")?
            .to_owned();
        if skipped(&relative) {
            continue;
        }
        let path = checked_path(root, &relative)?;
        let kind = entry
            .file_type()
            .map_err(|_| "Cannot inspect public-tree type")?;
        if kind.is_dir() {
            if private_path(&relative) {
                return Err(fail("Private path in public tree", &relative));
            }
            walk(root, &path, files)?;
        } else if kind.is_file() {
            files.insert(relative);
        } else {
            return Err(fail("Public path must be a regular file", &relative));
        }
    }
    Ok(())
}

/// Match Git's public candidate set: tracked files plus unignored new files.
/// Ignored, untracked local context must not be opened or exposed by the scan.
fn file_list(root: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    if !git_root(root)? {
        walk(root, root, &mut files)?;
        return Ok(files);
    }
    let output = git_command(
        root,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
    )
    .output()
    .map_err(|_| "Cannot list Git public-tree files")?;
    if !output.status.success() {
        return Err("Git public-tree enumeration failed".into());
    }
    for name in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        let name = std::str::from_utf8(name).map_err(|_| "Public file name is not valid UTF-8")?;
        let relative = PathBuf::from(name);
        // Unlike ignored local output, a Git candidate can enter the public tree.
        // Reject it before the ordinary target/ exclusion or any file reads.
        if relative
            .components()
            .any(|part| part.as_os_str() == "target")
        {
            return Err(fail("Tracked or non-ignored Rust build output", &relative));
        }
        if skipped(&relative) {
            continue;
        }
        let path = checked_path(root, &relative)?;
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() => {
                files.insert(relative);
            }
            // A tracked deletion is not public working-tree content to read.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(fail("Public path must be a regular file", &relative)),
        }
    }
    Ok(files)
}

fn rust_or_data(relative: &Path) -> bool {
    relative
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            [
                "rs", "md", "mdc", "json", "toml", "lock", "yml", "yaml", "txt",
            ]
            .contains(&extension)
        })
        || relative
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| {
                ["LICENSE", ".gitignore", ".gitattributes", ".env.example"].contains(&name)
            })
}

fn percent_decode(value: &str) -> Result<String> {
    let mut decoded = Vec::with_capacity(value.len());
    let mut source = value.as_bytes().iter().copied();
    while let Some(byte) = source.next() {
        if byte == b'%' {
            let high = source.next().and_then(|byte| (byte as char).to_digit(16));
            let low = source.next().and_then(|byte| (byte as char).to_digit(16));
            match (high, low) {
                (Some(high), Some(low)) => decoded.push((high * 16 + low) as u8),
                _ => return Err("Invalid percent encoding in local Markdown link".into()),
            }
        } else {
            decoded.push(byte);
        }
    }
    String::from_utf8(decoded).map_err(|_| "Local Markdown link is not valid UTF-8".into())
}

fn local_link(root: &Path, relative: &Path, target: &str) -> Result<()> {
    let scheme = target.split_once(':').is_some_and(|(scheme, _)| {
        scheme.starts_with(|character: char| character.is_ascii_alphabetic())
            && scheme.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
            })
    });
    if scheme || target.starts_with('#') || target.starts_with("//") {
        return Ok(());
    }
    // Split URI delimiters before decoding: an encoded # belongs to a filename.
    let raw = target.split(['?', '#']).next().unwrap_or("");
    if raw.is_empty() {
        return Ok(());
    }
    let decoded =
        percent_decode(raw).map_err(|_| fail("Invalid local Markdown link encoding", relative))?;
    let link = Path::new(&decoded);
    if link.is_absolute() || decoded.contains(['\\', '\0']) {
        return Err(fail("Broken or escaping local Markdown link", relative));
    }
    let joined = root.join(relative).parent().unwrap_or(root).join(link);
    no_symlink_ancestors(&joined)?;
    let canonical = fs::canonicalize(joined)
        .map_err(|_| fail("Broken or escaping local Markdown link", relative))?;
    if !canonical.starts_with(root) {
        return Err(fail("Broken or escaping local Markdown link", relative));
    }
    Ok(())
}

fn local_links(root: &Path, relative: &Path, text: &str) -> Result<()> {
    let parser = Parser::new(text);
    // Definitions are not events. Inspect them even when no usage refers to them.
    // The parser supplies CommonMark label normalization and decoded escapes.
    for (_, definition) in parser.reference_definitions().iter() {
        local_link(root, relative, &definition.dest)?;
    }
    for event in parser {
        if let Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) = event {
            local_link(root, relative, &dest_url)?;
        }
    }
    Ok(())
}

struct TextPatterns {
    secrets: Regex,
    emails: Regex,
}

fn field<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Value> {
    mapping.get(Value::String(key.into()))
}

fn no_yaml_merges(value: &Value) -> bool {
    match value {
        Value::Mapping(mapping) => {
            !mapping.contains_key(Value::String("<<".into()))
                && mapping.values().all(no_yaml_merges)
        }
        Value::Sequence(sequence) => sequence.iter().all(no_yaml_merges),
        Value::Tagged(_) => false,
        _ => true,
    }
}

fn readonly_permissions(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_mapping)
        .is_some_and(|permissions| {
            field(permissions, "contents").and_then(Value::as_str) == Some("read")
                && permissions
                    .values()
                    .all(|value| matches!(value.as_str(), Some("read" | "none")))
        })
}

fn pinned_action(action: &str) -> bool {
    action.split_once('@').is_some_and(|(name, pin)| {
        !name.is_empty()
            && !name.chars().any(char::is_whitespace)
            && pin.len() == 40
            && pin
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn rust_command(command: &str) -> bool {
    let command = command.trim();
    !command.is_empty()
        && !command.contains(['\r', '\n'])
        && command
            .split_whitespace()
            .next()
            .is_some_and(|program| matches!(program, "cargo" | "rustup"))
        && command.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    ' ' | '\t' | '-' | '_' | '.' | '/' | ':' | '=' | '+' | ','
                )
        })
}

fn supported_shell(mapping: &Mapping) -> bool {
    field(mapping, "shell").is_none_or(|shell| {
        matches!(
            shell.as_str(),
            Some("bash" | "sh" | "pwsh" | "powershell" | "cmd")
        )
    })
}

fn supported_default_shell(mapping: &Mapping) -> bool {
    field(mapping, "defaults")
        .and_then(Value::as_mapping)
        .and_then(|defaults| field(defaults, "run"))
        .and_then(Value::as_mapping)
        .is_none_or(supported_shell)
}

fn privileged_trigger(events: &Value) -> bool {
    let forbidden = |event: &str| matches!(event, "pull_request_target" | "workflow_run");
    match events {
        Value::String(event) => forbidden(event),
        Value::Sequence(events) => events.iter().any(privileged_trigger),
        Value::Mapping(events) => events
            .keys()
            .any(|event| event.as_str().is_some_and(forbidden)),
        _ => false,
    }
}

fn secret_access(value: &Value, context: &Regex) -> bool {
    match value {
        Value::Mapping(mapping) => {
            mapping.keys().any(|key| key.as_str() == Some("secrets"))
                || mapping.values().any(|value| secret_access(value, context))
        }
        Value::Sequence(sequence) => sequence.iter().any(|value| secret_access(value, context)),
        Value::String(text) => context.is_match(text),
        _ => false,
    }
}

fn workflow(relative: &Path, bytes: &[u8], secret_context: &Regex) -> Result<()> {
    let document: Value =
        serde_yaml_ng::from_slice(bytes).map_err(|_| "Invalid CI workflow YAML")?;
    if !no_yaml_merges(&document) {
        return Err("CI workflow must use explicit mappings without YAML merges or tags".into());
    }
    if secret_access(&document, secret_context) {
        return Err("CI secret access or forwarding requires separate review".into());
    }
    let document = document
        .as_mapping()
        .ok_or("CI workflow must be a mapping")?;
    if field(document, "on").is_some_and(privileged_trigger) {
        return Err("Privileged CI triggers require separate review".into());
    }
    if !readonly_permissions(field(document, "permissions")) {
        return Err("CI must have explicit read-only contents permissions".into());
    }
    if !supported_default_shell(document) {
        return Err("CI command shells must not select another implementation language".into());
    }
    let jobs = field(document, "jobs")
        .and_then(Value::as_mapping)
        .ok_or("CI jobs are missing")?;
    let mut checkout = false;
    for job in jobs.values() {
        let job = job.as_mapping().ok_or("Invalid CI job")?;
        if !supported_default_shell(job) {
            return Err("CI command shells must not select another implementation language".into());
        }
        if field(job, "permissions").is_some() && !readonly_permissions(field(job, "permissions")) {
            return Err("CI job permissions must preserve read-only contents access".into());
        }
        if let Some(action) = field(job, "uses")
            && !action.as_str().is_some_and(pinned_action)
        {
            return Err("External CI actions must use full commit pins".into());
        }
        if let Some(steps) = field(job, "steps") {
            if !field(job, "timeout-minutes")
                .and_then(Value::as_u64)
                .is_some_and(|minutes| (1..=10).contains(&minutes))
            {
                return Err("CI jobs must set timeout-minutes between 1 and 10".into());
            }
            let steps = steps.as_sequence().ok_or("CI steps must be a sequence")?;
            for step in steps {
                let step = step.as_mapping().ok_or("Invalid CI step")?;
                if let Some(command) = field(step, "run")
                    && (!command.as_str().is_some_and(rust_command) || !supported_shell(step))
                {
                    return Err("CI run steps must be single Rust tooling invocations".into());
                }
                if let Some(action) = field(step, "uses") {
                    let action = action.as_str().ok_or("CI action must be a string")?;
                    if !pinned_action(action) {
                        return Err("External CI actions must use full commit pins".into());
                    }
                    if action
                        .split('@')
                        .next()
                        .is_some_and(|name| name.eq_ignore_ascii_case("actions/checkout"))
                    {
                        checkout = true;
                        let setting = field(step, "with")
                            .and_then(Value::as_mapping)
                            .and_then(|mapping| field(mapping, "persist-credentials"));
                        if !matches!(setting, Some(Value::Bool(false)))
                            && setting.and_then(Value::as_str) != Some("false")
                        {
                            return Err("CI checkout must set persist-credentials: false".into());
                        }
                    }
                }
            }
        }
    }
    if !checkout && relative == Path::new(".github/workflows/validate.yml") {
        return Err("CI must include a pinned checkout with persisted credentials disabled".into());
    }
    Ok(())
}

/// Audit public paths, Rust-only sources, selected privacy patterns, links and CI.
/// This is a structural guard, not a guarantee that all sensitive data is detected.
pub fn check(root: &Path) -> Result<()> {
    // Do not silently audit the default index when Git is using an alternate
    // index: it may be the actual commit candidate for the caller.
    if std::env::var_os("GIT_INDEX_FILE").is_some() {
        return Err("GIT_INDEX_FILE override prevents verification of the commit index".into());
    }
    let root = checked_root(root)?;
    let patterns = TextPatterns {
        secrets: Regex::new(
            r"\b(?:gh[pousr]_|github_pat_|sk-(?:proj-|svcacct-)?|glpat-|xox[baprs]-)[A-Za-z0-9_-]{20,}|\b(?:AKIA|ASIA)[0-9A-Z]{16}\b|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----",
        )
        .map_err(|_| "Invalid credential scanner expression")?,
        emails: Regex::new(r"[\w.+-]+@([\w-]+(?:\.[\w-]+)+)")
            .map_err(|_| "Invalid contact scanner expression")?,
    };
    let secret_context = Regex::new(r"\$\{\{[^}]*\bsecrets\b")
        .map_err(|_| "Invalid CI secret context expression")?;
    let required_workflow = PathBuf::from(".github/workflows/validate.yml");
    if staged_deletions(&root)?.contains(&required_workflow) {
        return Err(fail(
            "Commit candidate contains staged deletion of required CI workflow",
            &required_workflow,
        ));
    }
    let staged = staged_entries(&root)?;
    let mut files = file_list(&root)?;
    if let Some(entries) = &staged {
        files.extend(entries.keys().cloned());
    }
    let mut workflows = Vec::new();
    for relative in files {
        if private_path(&relative) {
            return Err(fail("Private path in public tree", &relative));
        }
        if !rust_or_data(&relative) {
            return Err(fail("Executable source must be Rust", &relative));
        }
        let mut found_content = false;
        if let Some(entries) = &staged
            && let Some(oid) = entries.get(&relative)
        {
            let bytes = staged_blob(&root, oid, &relative)?;
            let text = String::from_utf8(bytes)
                .map_err(|_| fail("Public file must be readable UTF-8", &relative))?;
            inspect_text(&root, &relative, &text, &patterns, &mut workflows)?;
            found_content = true;
        }
        let path = checked_path(&root, &relative)?;
        match fs::read_to_string(path) {
            Ok(text) => {
                inspect_text(&root, &relative, &text, &patterns, &mut workflows)?;
                found_content = true;
            }
            Err(error) if found_content && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(fail("Public file must be readable UTF-8", &relative)),
        }
        if !found_content {
            return Err(fail("Public file must be readable UTF-8", &relative));
        }
    }
    if !workflows.iter().any(|(path, _)| path == &required_workflow) {
        return Err(fail("Missing CI workflow", &required_workflow));
    }
    for (relative, text) in workflows {
        workflow(&relative, text.as_bytes(), &secret_context)?;
    }
    Ok(())
}

fn inspect_text(
    root: &Path,
    relative: &Path,
    text: &str,
    patterns: &TextPatterns,
    workflows: &mut Vec<(PathBuf, String)>,
) -> Result<()> {
    let contact = patterns.emails.captures_iter(text).any(|matched| {
        let domain = matched[1].to_ascii_lowercase();
        domain != "example.invalid" && !domain.ends_with(".example.invalid")
    });
    if patterns.secrets.is_match(text) || contact {
        return Err(fail("Potential private credential/contact", relative));
    }
    if relative
        .extension()
        .is_some_and(|extension| extension == "md")
    {
        local_links(root, relative, text)?;
    }
    if relative.parent() == Some(Path::new(".github/workflows"))
        && relative
            .extension()
            .is_some_and(|extension| extension == "yml" || extension == "yaml")
    {
        workflows.push((relative.to_owned(), text.to_owned()));
    }
    Ok(())
}
