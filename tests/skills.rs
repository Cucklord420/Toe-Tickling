use std::{fs, path::Path};
use tempfile::TempDir;
use toe_tickling::skills::validate_skills;

const VALID: &str = "---\nname: sample-skill\ndescription: Use when validating a sample skill.\n---\n\n# Sample skill\n\nCheck the input and report failures.\n";

fn fixture(name: &str, content: Option<&str>) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let skill = dir.path().join(".agents/skills").join(name);
    fs::create_dir_all(&skill).unwrap();
    if let Some(content) = content {
        fs::write(skill.join("SKILL.md"), content).unwrap();
    }
    dir
}

#[test]
fn skill_directory_is_optional_when_no_skills_are_installed() {
    let dir = tempfile::tempdir().unwrap();
    validate_skills(dir.path()).unwrap();
}

#[test]
fn valid_skill_passes() {
    let dir = fixture("sample-skill", Some(VALID));
    validate_skills(dir.path()).unwrap();
}

#[test]
fn missing_skill_file_fails_closed() {
    let dir = fixture("sample-skill", None);
    assert!(validate_skills(dir.path()).is_err());
}

#[test]
fn missing_frontmatter_delimiter_fails_closed() {
    let dir = fixture("sample-skill", Some("name: sample-skill\n# Instructions\n"));
    assert!(validate_skills(dir.path()).is_err());
}

#[test]
fn mismatched_directory_and_declared_name_fails_closed() {
    let dir = fixture(
        "sample-skill",
        Some(&VALID.replace("name: sample-skill", "name: other-skill")),
    );
    assert!(validate_skills(dir.path()).is_err());
}

#[test]
fn invalid_names_fail_closed() {
    for name in ["Bad-Skill", "-bad", "bad--skill", "bad-"] {
        let dir = fixture(name, Some(VALID));
        assert!(validate_skills(dir.path()).is_err(), "{name}");
    }
}

#[test]
fn missing_or_oversized_description_fails_closed() {
    let dir = fixture(
        "sample-skill",
        Some(&VALID.replace(
            "description: Use when validating a sample skill.",
            "description: ''",
        )),
    );
    assert!(validate_skills(dir.path()).is_err());
    let long_description = "x".repeat(1025);
    let dir = fixture(
        "sample-skill",
        Some(&VALID.replace(
            "description: Use when validating a sample skill.",
            &format!("description: {long_description}"),
        )),
    );
    assert!(validate_skills(dir.path()).is_err());
}

#[test]
fn empty_instructions_fail_closed() {
    let dir = fixture(
        "sample-skill",
        Some("---\nname: sample-skill\ndescription: Use for a sample.\n---\n"),
    );
    assert!(validate_skills(dir.path()).is_err());
}

#[test]
fn malformed_yaml_fails_closed() {
    let dir = fixture(
        "sample-skill",
        Some("---\nname: [unclosed\ndescription: Use for a sample.\n---\n\n# Instructions\n"),
    );
    assert!(validate_skills(dir.path()).is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_skill_file_fails_closed() {
    use std::os::unix::fs::symlink;
    let dir = fixture("sample-skill", None);
    let outside = dir.path().join("outside.md");
    fs::write(&outside, VALID).unwrap();
    symlink(&outside, dir.path().join(".agents/skills/sample-skill/SKILL.md")).unwrap();
    assert!(validate_skills(dir.path()).is_err());
}

#[test]
fn repository_shopping_skill_is_checked() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let _skill = include_str!("../.agents/skills/shopping-research/SKILL.md");
    validate_skills(root).unwrap();
}
