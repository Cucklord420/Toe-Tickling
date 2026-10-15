use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};
use time::OffsetDateTime;
use toe_tickling::{
    Result, check_repo, current_manifest, load_policy, new_audit, read_json, render,
    validate_audit, validate_policy, write_json,
};

fn run() -> Result<()> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let root = env::current_dir()
        .map_err(|e| e.to_string())?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if args.is_empty() || args == ["--help"] {
        println!(
            "Run from the rule book checkout:\n  cargo run --locked -- check [--audit /private/receipt.json]\n  cargo run --locked -- render [--check]\n  cargo run --locked -- refresh-manifest\n  cargo run --locked -- new-audit --task 'Task description' --output /private/receipt.json\n\nChecks validate declarations and bytes, not truth or actual model behavior."
        );
        return Ok(());
    }
    let command = args.remove(0);
    let now = OffsetDateTime::now_utc();
    match command.as_str() {
        "check" => {
            let policy = check_repo(&root, now.date())?;
            if !args.is_empty() {
                if args.len() != 2 || args[0] != "--audit" {
                    return Err("Expected --audit PATH".into());
                }
                let audit = read_json(Path::new(&args[1]))?;
                validate_audit(&root, &policy, &audit, now)?;
                println!(
                    "Audit declarations complete; inspect underlying evidence and behavior separately."
                );
            }
            println!(
                "Repository structural checks passed ({} rules).",
                policy.rules.len()
            );
        }
        "render" => {
            if !args.is_empty() && args != ["--check"] {
                return Err("Expected render [--check]".into());
            }
            let policy = load_policy(&root)?;
            validate_policy(&policy, now.date())?;
            let text = render(&policy);
            let target = root.join("RULEBOOK.md");
            if args == ["--check"] {
                if fs::read_to_string(target).map_err(|e| e.to_string())? != text {
                    return Err("RULEBOOK.md is stale".into());
                }
            } else {
                fs::write(target, text).map_err(|e| e.to_string())?;
            }
            println!("Rule book render verified.");
        }
        "refresh-manifest" => {
            if !args.is_empty() {
                return Err("refresh-manifest takes no arguments".into());
            }
            // Explicit mutation command, never performed by a validation command.
            write_json(
                &root.join("policy/reading-manifest.json"),
                &current_manifest(&root)?,
            )?;
            println!("Reading manifest refreshed. Review the diff; this does not prove reading.");
        }
        "new-audit" => {
            if args.len() != 4 || args[0] != "--task" || args[2] != "--output" {
                return Err("Expected new-audit --task TEXT --output ABSOLUTE_PRIVATE_PATH".into());
            }
            let output = PathBuf::from(&args[3]);
            if !output.is_absolute() {
                return Err("Audit output must be absolute and outside this repository".into());
            }
            let parent = output
                .parent()
                .ok_or("Missing parent directory")?
                .canonicalize()
                .map_err(|e| format!("Create the private output directory first: {e}"))?;
            if parent.starts_with(&root) {
                return Err("Keep real audit records outside the repository".into());
            }
            let audit = new_audit(&root, args[1].clone())?;
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(&output)
                .map_err(|e| format!("Cannot create {}: {e}", output.display()))?;
            let text = serde_json::to_string_pretty(&audit).map_err(|e| e.to_string())? + "\n";
            file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            println!(
                "Created blocked template at {}. Fill with actual evidence; no file was overwritten.",
                output.display()
            );
        }
        _ => return Err(format!("Unknown command: {command}; use --help")),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
}
