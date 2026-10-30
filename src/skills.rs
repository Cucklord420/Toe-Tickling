//! Deterministic Agent Skills packaging checks; not a model-behavior test.
use std::{fs, io::ErrorKind, path::Path};

use serde_yaml_ng::Value;

use crate::Result;

fn invalid(path: &Path, message: &str) -> String {
    format!("{}: {message}", path.display())
}

fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes.first() != Some(&b'-')
        && bytes.last() != Some(&b'-')
        && !bytes.windows(2).any(|pair| pair == b"--")
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn text_field<'a>(mapping: &'a serde_yaml_ng::Mapping, name: &str) -> Option<&'a str> {
    mapping
        .get(Value::String(name.to_owned()))
        .and_then(Value::as_str)
}

/// Validate every local Agent Skills package without executing skill instructions.
///
/// A missing skills directory is allowed for repositories without installed skills.
/// Once the directory exists, empty, malformed or unsafe entries fail closed.
pub fn validate_skills(root: &Path) -> Result<()> {
    let base = root.join(".agents/skills");
    let metadata = match fs::symlink_metadata(&base) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("{}: {error}", base.display())),
    };

    let agents = root.join(".agents");
    let agents_meta =
        fs::symlink_metadata(&agents).map_err(|error| format!("{}: {error}", agents.display()))?;
    if !agents_meta.file_type().is_dir() || !metadata.file_type().is_dir() {
        return Err(invalid(
            &base,
            "Agent Skills directory must not be a symlink or file",
        ));
    }

    let mut count = 0usize;
    for entry in fs::read_dir(&base).map_err(|error| format!("{}: {error}", base.display()))? {
        let entry = entry.map_err(|error| format!("{}: {error}", base.display()))?;
        let directory = entry.path();
        let kind = entry
            .file_type()
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        if !kind.is_dir() {
            return Err(invalid(&directory, "skill entry must be a real directory"));
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid(&directory, "skill directory name is not UTF-8"))?;
        if !valid_name(&name) {
            return Err(invalid(&directory, "invalid Agent Skills directory name"));
        }

        let file = directory.join("SKILL.md");
        let file_meta =
            fs::symlink_metadata(&file).map_err(|error| format!("{}: {error}", file.display()))?;
        if !file_meta.file_type().is_file() {
            return Err(invalid(&file, "SKILL.md must be a real regular file"));
        }
        let contents =
            fs::read_to_string(&file).map_err(|error| format!("{}: {error}", file.display()))?;
        let normalized = contents.replace("\r\n", "\n");
        let rest = normalized
            .strip_prefix("---\n")
            .ok_or_else(|| invalid(&file, "missing opening YAML frontmatter delimiter"))?;
        let (frontmatter, body) = rest
            .split_once("\n---\n")
            .ok_or_else(|| invalid(&file, "missing closing YAML frontmatter delimiter"))?;
        let yaml: Value = serde_yaml_ng::from_str(frontmatter)
            .map_err(|error| format!("{}: invalid YAML: {error}", file.display()))?;
        let mapping = yaml
            .as_mapping()
            .ok_or_else(|| invalid(&file, "frontmatter must be a YAML mapping"))?;

        let declared =
            text_field(mapping, "name").ok_or_else(|| invalid(&file, "missing string name"))?;
        if declared != name {
            return Err(invalid(
                &file,
                "declared skill name must match its directory",
            ));
        }
        let description = text_field(mapping, "description")
            .ok_or_else(|| invalid(&file, "missing string description"))?;
        if description.trim().is_empty() || description.chars().count() > 1024 {
            return Err(invalid(&file, "description must contain 1-1024 characters"));
        }
        if body.trim().is_empty() {
            return Err(invalid(&file, "skill instructions must not be empty"));
        }

        for field in ["license", "compatibility", "allowed-tools"] {
            if mapping.contains_key(Value::String(field.to_owned()))
                && text_field(mapping, field).is_none()
            {
                return Err(invalid(
                    &file,
                    "optional frontmatter field must be a string",
                ));
            }
        }
        if let Some(compatibility) = text_field(mapping, "compatibility")
            && (compatibility.trim().is_empty() || compatibility.chars().count() > 500)
        {
            return Err(invalid(
                &file,
                "compatibility must contain 1-500 characters",
            ));
        }
        if let Some(extra) = mapping.get(Value::String("metadata".to_owned())) {
            let values = extra
                .as_mapping()
                .ok_or_else(|| invalid(&file, "metadata must be a mapping"))?;
            if values
                .iter()
                .any(|(key, value)| key.as_str().is_none() || value.as_str().is_none())
            {
                return Err(invalid(&file, "metadata keys and values must be strings"));
            }
        }
        count += 1;
    }
    if count == 0 {
        return Err(invalid(
            &base,
            "skills directory exists but contains no skills",
        ));
    }
    Ok(())
}
