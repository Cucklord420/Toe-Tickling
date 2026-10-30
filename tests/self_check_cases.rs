//! Verifies skill acceptance-case contracts, not agent/model responses to the cases.
use std::{collections::BTreeSet, fs, path::Path};

fn validate(root: &Path) -> Result<usize, String> {
    let directory = root.join(".agents/skills");
    let mut skills = 0usize;
    for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Non-UTF-8 skill name".to_string())?;
        let markdown =
            fs::read_to_string(path.join("SKILL.md")).map_err(|error| error.to_string())?;
        if !markdown.contains("## Self-checks") {
            return Err(format!("{name}: missing self-check procedure"));
        }
        let cases_file = path.join("references/self-check-cases.json");
        let meta = fs::symlink_metadata(&cases_file).map_err(|error| error.to_string())?;
        if !meta.file_type().is_file() {
            return Err(format!("{name}: acceptance file is not a regular file"));
        }
        let data: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&cases_file).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if data["schema_version"] != 1 || data["skill"] != name {
            return Err(format!(
                "{name}: incompatible acceptance schema or skill name"
            ));
        }
        let cases = data["cases"]
            .as_array()
            .ok_or_else(|| format!("{name}: missing case array"))?;
        if !(5..=64).contains(&cases.len()) {
            return Err(format!(
                "{name}: expected five or more distinct acceptance cases"
            ));
        }
        let mut ids = BTreeSet::new();
        let mut positives = 0;
        let mut negatives = 0;
        for case in cases {
            let id = case["id"]
                .as_str()
                .ok_or_else(|| format!("{name}: missing case identifier"))?;
            if !ids.insert(id) {
                return Err(format!("{name}: duplicate case {id}"));
            }
            for key in ["scenario", "required", "forbidden"] {
                if !case[key]
                    .as_str()
                    .is_some_and(|value| value.trim().len() >= 24)
                {
                    return Err(format!("{name}: incomplete {key} for case {id}"));
                }
            }
            match case["kind"].as_str() {
                Some("positive") => positives += 1,
                Some("negative") => negatives += 1,
                _ => return Err(format!("{name}: invalid case kind for {id}")),
            }
        }
        if positives == 0 || negatives == 0 {
            return Err(format!("{name}: needs positive and negative scenarios"));
        }
        skills += 1;
    }
    if skills == 0 {
        return Err("No skills found for the acceptance suite".into());
    }
    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fixture() -> TempDir {
        let directory = tempfile::tempdir().unwrap();
        let skill = directory.path().join(".agents/skills/example");
        fs::create_dir_all(skill.join("references")).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: example\ndescription: Example test fixture skill.\n---\n## Self-checks\nVerify actual behavior.\n",
        )
        .unwrap();
        let cases: Vec<_> = (0..5)
            .map(|index| {
                serde_json::json!({
                    "id": format!("case-{index}"),
                    "kind": if index == 0 { "positive" } else { "negative" },
                    "scenario": "Representative fixture with an observable edge-case input",
                    "required": "Report actual evidence and refuse unsupported successful outcomes",
                    "forbidden": "Never assert that an unperformed verification actually passed"
                })
            })
            .collect();
        fs::write(
            skill.join("references/self-check-cases.json"),
            serde_json::json!({"schema_version": 1, "skill": "example", "cases": cases})
                .to_string(),
        )
        .unwrap();
        directory
    }

    #[test]
    fn real_repo_skill_cases_are_complete() {
        assert!(validate(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap() >= 2);
    }

    #[test]
    fn valid_synthetic_cases_pass() {
        assert_eq!(validate(fixture().path()).unwrap(), 1);
    }

    #[test]
    fn missing_case_file_is_rejected() {
        let directory = fixture();
        fs::remove_file(
            directory
                .path()
                .join(".agents/skills/example/references/self-check-cases.json"),
        )
        .unwrap();
        assert!(validate(directory.path()).is_err());
    }

    #[test]
    fn duplicate_cases_are_rejected() {
        let directory = fixture();
        let file = directory
            .path()
            .join(".agents/skills/example/references/self-check-cases.json");
        let mut data: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        data["cases"][1]["id"] = serde_json::json!("case-0");
        fs::write(file, data.to_string()).unwrap();
        assert!(
            validate(directory.path())
                .unwrap_err()
                .contains("duplicate")
        );
    }

    #[test]
    fn missing_negative_cases_are_rejected() {
        let directory = fixture();
        let file = directory
            .path()
            .join(".agents/skills/example/references/self-check-cases.json");
        let mut data: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        for case in data["cases"].as_array_mut().unwrap() {
            case["kind"] = serde_json::json!("positive");
        }
        fs::write(file, data.to_string()).unwrap();
        assert!(
            validate(directory.path())
                .unwrap_err()
                .contains("positive and negative")
        );
    }

    #[test]
    fn insufficient_case_assertions_are_rejected() {
        let directory = fixture();
        let file = directory
            .path()
            .join(".agents/skills/example/references/self-check-cases.json");
        let mut data: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        data["cases"][1]["required"] = serde_json::json!("");
        fs::write(file, data.to_string()).unwrap();
        assert!(
            validate(directory.path())
                .unwrap_err()
                .contains("incomplete")
        );
    }

    #[test]
    fn missing_self_check_instructions_are_rejected() {
        let directory = fixture();
        let file = directory.path().join(".agents/skills/example/SKILL.md");
        let body = fs::read_to_string(&file)
            .unwrap()
            .replace("## Self-checks", "## General Notes");
        fs::write(file, body).unwrap();
        assert!(
            validate(directory.path())
                .unwrap_err()
                .contains("self-check")
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_case_file_is_rejected() {
        use std::os::unix::fs::symlink;
        let directory = fixture();
        let file = directory
            .path()
            .join(".agents/skills/example/references/self-check-cases.json");
        let outside = directory.path().join("outside.json");
        fs::rename(&file, &outside).unwrap();
        symlink(outside, file).unwrap();
        assert!(
            validate(directory.path())
                .unwrap_err()
                .contains("regular file")
        );
    }
}
