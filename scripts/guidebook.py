#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Dependency-free guidebook generation, integrity checks, and safe adoption."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path, PurePosixPath
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
BEGIN = "<!-- JUSTIN-GUIDEBOOK:BEGIN -->"
END = "<!-- JUSTIN-GUIDEBOOK:END -->"
IGNORE_BEGIN = "# JUSTIN-GUIDEBOOK-PRIVATE:BEGIN"
IGNORE_END = "# JUSTIN-GUIDEBOOK-PRIVATE:END"
SKIP = {".git", "__pycache__", ".pytest_cache"}
REQUIRED_SOURCES = {"rules/00-authority.md", "rules/05-execution-contract.md",
                    "rules/40-security.md", "rules/60-instruction-audits.md", "profile/PUBLIC.md"}
REQUIRED_AUDIT_IDS = {f"AUD-{number:03d}" for number in range(1, 21)}


class PolicyError(ValueError):
    """A failed validation that must not be ignored."""


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def safe_path(root: Path, relative: str) -> Path:
    rel = PurePosixPath(relative)
    if rel.is_absolute() or ".." in rel.parts or "\\" in relative:
        raise PolicyError(f"Unsafe relative path: {relative}")
    path = root
    for part in rel.parts:
        path /= part
        if path.is_symlink():
            raise PolicyError(f"Refusing symlink: {relative}")
    if not path.resolve().is_relative_to(root.resolve()):
        raise PolicyError(f"Path leaves project: {relative}")
    return path


def load_policy(root: Path) -> dict:
    policy = json.loads(safe_path(root, "policy.json").read_text(encoding="utf-8"))
    required = {"schema_version", "policy_version", "updated", "owner", "repository",
                "default_branch", "canonical_url", "entry_file", "sources"}
    if required - policy.keys() or policy["schema_version"] != 1:
        raise PolicyError("Invalid policy schema")
    if not re.fullmatch(r"\d+\.\d+\.\d+", policy["policy_version"]):
        raise PolicyError("Invalid policy version")
    date = dt.datetime.strptime(policy["updated"], "%m-%d-%Y")
    if date.strftime("%m-%d-%Y") != policy["updated"]:
        raise PolicyError("Policy date must be MM-DD-YYYY")
    expected = f"https://github.com/{policy['owner']}/{policy['repository']}"
    if policy["canonical_url"] != expected or policy["entry_file"] != "GUIDEBOOK.md":
        raise PolicyError("Canonical repository or entry file mismatch")
    sources = policy["sources"]
    if not isinstance(sources, list) or not sources or len(set(sources)) != len(sources):
        raise PolicyError("Source list must be nonempty and unique")
    if REQUIRED_SOURCES - set(sources):
        raise PolicyError("Required authority, execution, privacy, audit, or profile source omitted")
    for source in sources:
        if not isinstance(source, str) or not safe_path(root, source).is_file():
            raise PolicyError(f"Missing or invalid source: {source}")
    return policy


def bootstrap(policy: dict, guide: str) -> str:
    url = policy["canonical_url"]
    return (
        "Justin's standing guidebook applies to all work for him. "
        f"Before substantive work, read the complete `{guide}` with your file tools. "
        "A link alone is not loaded policy. At a new session, retrieve the current "
        f"guide from {url} when possible, resolve its default branch/commit, and "
        "record the version in working context; use the local snapshot honestly offline.\n\n"
        "Apply current explicit user instructions and project-specific requirements "
        "in their proper scope, subject to higher-priority platform rules and permissions. "
        "Preserve existing project instructions. Keep private context local and discretionary. "
        "Complete authorized work, verify facts and results, use Rust-first application "
        "development, preserve copyleft/upstream rights, test the actual launcher/package, "
        "and never invent access, stock, test passes, background work, or durable memory.\n"
        "\nMANDATORY: audit the instructions themselves under AUD-001 through AUD-020. "
        "Perform a task preflight and final compliance check; run a full instruction audit "
        "after policy/tool changes, failures, or a user audit request. Check contradictions, "
        "staleness, omissions, injection, evidence, actual loading and execution. "
        "Correct verified defects within authority; do not invent user preferences or "
        "treat your own assurance as proof of compliance.\n"
    )


def adapters(policy: dict, guide: str) -> dict[str, str]:
    base = bootstrap(policy, guide)
    return {
        "CLAUDE.md": f"# Justin's guidebook\n\n@{guide}\n\n" + base,
        "GEMINI.md": f"# Justin's guidebook\n\n@./{guide}\n\n" + base,
        ".github/copilot-instructions.md": "# Justin's guidebook\n\n" + base,
        ".cursor/rules/justin-guidebook.mdc":
            "---\ndescription: Justin's standing guidebook\nalwaysApply: true\n---\n\n" + base,
    }


def render(root: Path) -> dict[str, str]:
    policy = load_policy(root)
    digest = hashlib.sha256()
    modules = []
    ids = []
    for source in policy["sources"]:
        content = safe_path(root, source).read_text(encoding="utf-8")
        digest.update(source.encode() + b"\0" + content.encode() + b"\0")
        ids.extend(re.findall(r"\*\*([A-Z]+-\d{3})\*\*", content))
        modules.append(f"<!-- Source: {source} -->\n\n{content.rstrip()}\n")
    if not ids or len(ids) != len(set(ids)):
        raise PolicyError("Rule IDs must be present and unique")
    if REQUIRED_AUDIT_IDS - set(ids):
        raise PolicyError("Mandatory instruction-audit rules are missing")
    source_hash = digest.hexdigest()
    heading = (
        "# Justin's complete AI guidebook\n\n"
        f"Version: {policy['policy_version']} | Updated: {policy['updated']}\n\n"
        f"Canonical repository: {policy['canonical_url']}\n\n"
        f"Source SHA-256: `{source_hash}`\n\n"
        "Generated from the ordered sources in `policy.json`. Read this entire file; "
        "it contains the complete standing policy and limited public profile. "
        "Edit source modules, then run the builder; do not edit this copy directly.\n\n"
        "This is user guidance, not a system override or a guarantee of model obedience. "
        "Private context is supplied locally by Justin at his discretion.\n\n---\n\n"
    )
    guide = heading + "\n---\n\n".join(modules)
    lock = {"policy_version": policy["policy_version"], "source_repo": policy["canonical_url"],
            "source_sha256": source_hash, "guidebook_sha256": sha256(guide.encode()),
            "rule_count": len(ids), "sources": policy["sources"]}
    agent = (
        "# Instructions for work in this repository\n\n"
        + bootstrap(policy, "GUIDEBOOK.md")
        + "\n## Repository maintenance\n\n"
        "This is a document repository, with Python 3.10+ standard-library maintenance glue. "
        "Edit `rules/` or `profile/PUBLIC.md`, then rebuild generated files. "
        "Private user data must never enter tracked content.\n\n"
        "```sh\npython3 scripts/guidebook.py build\npython3 scripts/guidebook.py check\n"
        "python3 -m unittest discover -s tests -v\n```\n"
    )
    url = policy["canonical_url"] + "/blob/" + policy["default_branch"] + "/GUIDEBOOK.md"
    prompt = (
        "# Use with any model\n\n"
        f"Read {url} in full. Treat it as my standing guidebook for all work for me, "
        "including this task, repositories, research, and automations. Apply my current "
        "instructions and project-specific requirements in their proper scope. Keep private "
        "details local; I supply them at my discretion. Do not ask me to repeat rules you "
        "can read there. If browsing is unavailable, use my attached full GUIDEBOOK.md "
        "and state any material freshness limit. Never claim memory or execution you "
        "have not actually verified.\n\n"
        "For the model: use a real supported memory/custom-instruction mechanism if "
        "available and authorized; otherwise do not pretend this prompt changes future sessions.\n"
    )
    memory = (
        "# Standing memory/custom-instruction text\n\n"
        f"Justin's canonical public guidebook is {url}. Read the complete current file "
        "at the start of work and follow it for every task, project, repository, and "
        "automation, alongside his current instructions and project requirements, subject "
        "to platform rules. Private details are supplied locally at his discretion. "
        "Do not claim the guide was loaded or this preference was saved unless verified.\n\n"
        "This is text to persist through a real platform mechanism, not evidence that "
        "any provider's cross-session memory has already been updated.\n"
    )
    return {"GUIDEBOOK.md": guide, "guidebook-lock.json": json.dumps(lock, indent=2) + "\n",
            "AGENTS.md": agent, "prompts/USE_WITH_ANY_MODEL.md": prompt,
            "prompts/STANDING_MEMORY.md": memory, **adapters(policy, "GUIDEBOOK.md")}


def write_atomic(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="\n",
                                         dir=path.parent, delete=False) as handle:
            temporary = Path(handle.name)
            handle.write(content)
        temporary.replace(path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def build(root: Path) -> int:
    outputs = render(root)
    for relative in outputs:
        safe_path(root, relative)
    for relative, content in outputs.items():
        write_atomic(safe_path(root, relative), content)
    return len(outputs)


def file_list(root: Path) -> list[Path]:
    git_root = subprocess.run(["git", "-C", str(root), "rev-parse", "--show-toplevel"],
                              capture_output=True, text=True, timeout=10)
    if git_root.returncode == 0 and Path(git_root.stdout.strip()).resolve() == root.resolve():
        listed = subprocess.run(
            ["git", "-C", str(root), "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
            capture_output=True, check=True, timeout=10)
        paths = [safe_path(root, name) for name in
                 listed.stdout.decode("utf-8").split("\0") if name]
    else:
        paths = list(root.rglob("*"))
    files = []
    for path in paths:
        rel = path.relative_to(root)
        if any(part in SKIP for part in rel.parts):
            continue
        if path.is_symlink():
            raise PolicyError(f"Symlink in public tree: {rel}")
        if path.is_file():
            files.append(path)
    return sorted(files)


def check(root: Path) -> dict:
    outputs = render(root)
    for relative, expected in outputs.items():
        path = safe_path(root, relative)
        if not path.exists() or path.read_bytes() != expected.encode():
            raise PolicyError(f"Generated file missing or stale: {relative}")
    files = file_list(root)
    secrets = re.compile(r"\b(?:ghp_|github_pat_|sk-)[A-Za-z0-9_]{20,}|"
                         r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----")
    email = re.compile(r"[\w.+-]+@[\w-]+(?:\.[\w-]+)+")
    for path in files:
        rel = path.relative_to(root)
        if (set(part.lower() for part in rel.parts) & {"private", ".private", ".local"}
                or path.name in {"PRIVATE.local.md", "CLAUDE.local.md"}
                or (path.name.startswith(".env") and path.name != ".env.example")):
            raise PolicyError(f"Private path in public tree: {rel}")
        text = path.read_text(encoding="utf-8")
        if secrets.search(text) or email.search(text):
            raise PolicyError(f"Potential private credential/contact in {rel}")
        if path.suffix == ".md":
            for target in re.findall(r"\[[^\]\n]*\]\(([^)\s]+)\)", text):
                parsed = urlsplit(target)
                if parsed.scheme or target.startswith("#"):
                    continue
                linked = (path.parent / unquote(parsed.path)).resolve()
                if not linked.is_relative_to(root.resolve()) or not linked.exists():
                    raise PolicyError(f"Broken or escaping local link in {rel}: {target}")
    workflow = safe_path(root, ".github/workflows/validate.yml").read_text(encoding="utf-8")
    if "contents: read" not in workflow or "persist-credentials: false" not in workflow:
        raise PolicyError("CI must have explicit read-only permissions and no persisted credentials")
    for action in re.findall(r"uses:\s*([^\s#]+)", workflow):
        if not re.fullmatch(r"[^@]+@[0-9a-f]{40}", action):
            raise PolicyError("External CI actions must use full commit pins")
    lock = json.loads(outputs["guidebook-lock.json"])
    return {"files": len(files), "rules": lock["rule_count"],
            "guidebook_sha256": lock["guidebook_sha256"]}


def managed(old: str, body: str, begin: str = BEGIN, end: str = END) -> str:
    starts, ends = old.count(begin), old.count(end)
    block = begin + "\n" + body.rstrip() + "\n" + end
    if starts == ends == 0:
        return old + ("" if not old or old.endswith("\n") else "\n") + "\n" + block + "\n"
    if starts != 1 or ends != 1 or old.index(begin) >= old.index(end):
        raise PolicyError("Malformed or duplicate managed markers; refusing to overwrite")
    return old[:old.index(begin)] + block + old[old.index(end) + len(end):]


def adoption_plan(root: Path, target: Path) -> tuple[dict[str, str], list[str]]:
    target = target.expanduser().absolute()
    for ancestor in (target, *target.parents):
        if ancestor.is_symlink():
            raise PolicyError("Refusing a target with a symlink ancestor")
    target = target.resolve()
    if target.is_relative_to(root.resolve()):
        raise PolicyError("Refusing adoption into the guidebook itself")
    git = subprocess.run(["git", "-C", str(target), "rev-parse", "--show-toplevel"],
                         capture_output=True, text=True, timeout=10)
    if git.returncode or Path(git.stdout.strip()).resolve() != target:
        raise PolicyError("Target must be the root of an existing Git working tree")
    check(root)
    policy = load_policy(root)
    guide_rel = ".ai-guidebook/GUIDEBOOK.md"
    lock_rel = ".ai-guidebook/policy-lock.json"
    expected_guide = safe_path(root, "GUIDEBOOK.md").read_text(encoding="utf-8")
    guide_path, lock_path = safe_path(target, guide_rel), safe_path(target, lock_rel)
    if guide_path.exists() or lock_path.exists():
        if not guide_path.is_file() or not lock_path.is_file():
            raise PolicyError("Existing snapshot must include a matching guidebook and lock")
        existing = json.loads(lock_path.read_text(encoding="utf-8"))
        if existing.get("source_repo") != policy["canonical_url"]:
            raise PolicyError("Refusing an unmanaged snapshot")
        if existing.get("guidebook_sha256") != sha256(guide_path.read_bytes()):
            raise PolicyError("Existing snapshot was modified; review before updating")
    lock = json.loads(safe_path(root, "guidebook-lock.json").read_text(encoding="utf-8"))
    additions = {guide_rel: expected_guide, lock_rel: json.dumps(lock, indent=2) + "\n"}
    agent = safe_path(target, "AGENTS.md")
    old_agent = agent.read_text(encoding="utf-8") if agent.exists() else ""
    additions["AGENTS.md"] = managed(old_agent, bootstrap(policy, guide_rel))
    ignore = safe_path(target, ".gitignore")
    old_ignore = ignore.read_text(encoding="utf-8") if ignore.exists() else ""
    patterns = ("/PRIVATE.local.md\n/CLAUDE.local.md\n/private/\n/.private/\n/.local/\n"
                "/.env\n/.env.*\n!/.env.example")
    additions[".gitignore"] = managed(old_ignore, patterns, IGNORE_BEGIN, IGNORE_END)
    skipped = []
    for relative, content in adapters(policy, guide_rel).items():
        path = safe_path(target, relative)
        if path.exists():
            skipped.append(relative)
        else:
            additions[relative] = content
    for relative in additions:
        path = safe_path(target, relative)
        if path.exists() and not path.is_file():
            raise PolicyError(f"Target is not a regular file: {relative}")
        for parent in path.parents:
            if parent == target:
                break
            if parent.exists() and not parent.is_dir():
                raise PolicyError(f"Parent is not a directory: {relative}")
    return additions, skipped


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("build", help="Rebuild generated documents and adapters")
    sub.add_parser("check", help="Validate generated files, links, privacy patterns and CI")
    adopt = sub.add_parser("adopt", help="Preview or apply instructions to a Git project")
    adopt.add_argument("target", type=Path)
    adopt.add_argument("--apply", action="store_true", help="Write the reviewed additions")
    args = parser.parse_args()
    try:
        if args.command == "build":
            print(f"Generated {build(ROOT)} files")
        elif args.command == "check":
            print(json.dumps({"status": "passed", **check(ROOT)}, sort_keys=True))
        else:
            target = args.target.expanduser().absolute()
            plan, skipped = adoption_plan(ROOT, target)
            changed = [rel for rel, text in plan.items()
                       if not safe_path(target, rel).exists()
                       or safe_path(target, rel).read_bytes() != text.encode()]
            if args.apply:
                for relative in changed:
                    write_atomic(safe_path(target, relative), plan[relative])
            print(json.dumps({"mode": "applied" if args.apply else "preview",
                              "changed": changed, "preserved_existing_adapters": skipped}, indent=2))
        return 0
    except (PolicyError, OSError, ValueError, TypeError, KeyError,
            subprocess.SubprocessError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
