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
    let selected = if let Some(index) = args.iter().position(|arg| arg == "--root") {
        if index + 1 >= args.len() {
            return Err("--root needs a path".into());
        }
        let value = args.remove(index + 1);
        args.remove(index);
        PathBuf::from(value)
    } else {
        env::current_dir().map_err(|e| e.to_string())?
    };
    let root = toe_tickling::security::checked_root(&selected)?;
    if args.is_empty() || args == ["--help"] {
        println!(
            "Use the checkout as current directory or pass --root PATH:\n  cargo run --locked -- build\n  cargo run --locked -- check [--audit /private/receipt.json]\n  cargo run --locked -- render [--check]\n  cargo run --locked -- refresh-manifest\n  cargo run --locked -- new-audit --task 'Task description' --output /private/receipt.json\n  cargo run --locked -- adopt /path/to/project [--apply]\n\nChecks validate declarations and bytes, not truth or actual model behavior."
        );
        return Ok(());
    }
    let command = args.remove(0);
    let now = OffsetDateTime::now_utc();
    match command.as_str() {
        "build" => {
            if !args.is_empty() {
                return Err("build takes no arguments".into());
            }
            println!(
                "Generated {} full-guide, register, and adapter artifacts.",
                toe_tickling::guide::build(&root)?
            );
        }
        "adopt" => {
            if args.is_empty() || args.len() > 2 || (args.len() == 2 && args[1] != "--apply") {
                return Err("Expected adopt TARGET [--apply]".into());
            }
            let plan = toe_tickling::adopt::plan(&root, Path::new(&args[0]))?;
            let changed: Vec<_> = plan
                .files
                .iter()
                .filter(|(path, text)| {
                    fs::read(plan.target.join(path)).ok().as_deref() != Some(text.as_bytes())
                })
                .map(|(path, _)| path.clone())
                .collect();
            let apply = args.len() == 2;
            if apply {
                toe_tickling::adopt::apply(&plan)?;
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "mode": if apply {"applied"} else {"preview"}, "changed": changed,
                    "preserved_existing_adapters": plan.preserved_adapters
                }))
                .map_err(|e| e.to_string())?
            );
        }
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
                "Repository structural checks passed ({} standing rules + {} verification rules).",
                toe_tickling::guide::module_ids(&root)?.len(),
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
                toe_tickling::guide::check(&root)?;
            } else {
                toe_tickling::guide::build(&root)?;
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
