# SPDX-License-Identifier: AGPL-3.0-only
"""Regression coverage for real maintenance entry points and safe adoption."""

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SOURCE = Path(__file__).resolve().parents[1]


class GuidebookTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="guidebook tests ")
        self.addCleanup(self.tmp.cleanup)
        self.work = Path(self.tmp.name)
        self.repo = self.work / "guidebook source with spaces"
        shutil.copytree(SOURCE, self.repo, ignore=shutil.ignore_patterns(
            ".git", "__pycache__", ".pytest_cache"))
        self.target = self.work / "existing project with spaces"
        self.target.mkdir()
        subprocess.run(["git", "init", "-q", str(self.target)], check=True,
                       capture_output=True, timeout=10)

    def cli(self, *arguments, ok=True):
        result = subprocess.run(
            [sys.executable, str(self.repo / "scripts/guidebook.py"), *map(str, arguments)],
            cwd=self.work, text=True, capture_output=True, timeout=30)
        if ok:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        return result

    def alter_policy(self, **updates):
        path = self.repo / "policy.json"
        data = json.loads(path.read_text())
        data.update(updates)
        path.write_text(json.dumps(data))

    def snapshot(self, root):
        return {str(p.relative_to(root)): p.read_bytes()
                for p in root.rglob("*") if p.is_file() and ".git" not in p.parts}

    def test_real_cli_build_and_check_from_unrelated_directory(self):
        self.cli("build")
        result = json.loads(self.cli("check").stdout)
        self.assertEqual(result["status"], "passed")
        self.assertGreaterEqual(result["rules"], 70)

    def test_build_is_deterministic(self):
        self.cli("build")
        before = self.snapshot(self.repo)
        self.cli("build")
        self.assertEqual(before, self.snapshot(self.repo))

    def test_generated_guide_tamper_is_rejected(self):
        with (self.repo / "GUIDEBOOK.md").open("a") as stream:
            stream.write("altered\n")
        self.assertIn("stale", self.cli("check", ok=False).stderr)

    def test_generated_adapter_tamper_is_rejected(self):
        (self.repo / "CLAUDE.md").write_text("unrelated\n")
        self.cli("check", ok=False)

    def test_missing_generated_guide_is_rejected(self):
        (self.repo / "GUIDEBOOK.md").unlink()
        self.cli("check", ok=False)

    def test_changed_source_requires_rebuild(self):
        with (self.repo / "rules/10-communication.md").open("a") as stream:
            stream.write("\nClarification for this disposable fixture.\n")
        self.cli("check", ok=False)
        self.cli("build")
        self.cli("check")

    def test_invalid_date_is_rejected(self):
        self.alter_policy(updated="2026-10-15")
        self.cli("build", ok=False)

    def test_invalid_version_is_rejected(self):
        self.alter_policy(policy_version="latest")
        self.cli("build", ok=False)

    def test_duplicate_rule_id_is_rejected(self):
        path = self.repo / "rules/10-communication.md"
        path.write_text(path.read_text() + "\n- **COM-001** duplicate\n")
        self.assertIn("unique", self.cli("build", ok=False).stderr)

    def test_missing_source_is_rejected(self):
        (self.repo / "rules/10-communication.md").unlink()
        self.cli("build", ok=False)

    def test_source_traversal_is_rejected(self):
        data = json.loads((self.repo / "policy.json").read_text())
        self.alter_policy(sources=data["sources"] + ["../outside.md"])
        self.assertIn("Unsafe", self.cli("build", ok=False).stderr)

    def test_audit_module_cannot_be_silently_omitted(self):
        data = json.loads((self.repo / "policy.json").read_text())
        self.alter_policy(sources=[source for source in data["sources"]
                                   if source != "rules/60-instruction-audits.md"])
        self.assertIn("audit", self.cli("build", ok=False).stderr)

    def test_mandatory_audit_rule_cannot_be_removed(self):
        path = self.repo / "rules/60-instruction-audits.md"
        path.write_text(path.read_text().replace("**AUD-003**", "**RENAMED-003**"))
        self.assertIn("audit", self.cli("build", ok=False).stderr)

    def test_all_adapters_include_mandatory_instruction_audit(self):
        for relative in ("AGENTS.md", "CLAUDE.md", "GEMINI.md",
                         ".github/copilot-instructions.md", ".cursor/rules/justin-guidebook.mdc"):
            self.assertIn("audit the instructions themselves", (self.repo / relative).read_text())

    @unittest.skipIf(os.name == "nt", "Symlink permissions depend on Windows setup")
    def test_source_symlink_is_rejected(self):
        path = self.repo / "rules/10-communication.md"
        other = self.work / "outside.md"
        other.write_text(path.read_text())
        path.unlink()
        path.symlink_to(other)
        self.assertIn("symlink", self.cli("build", ok=False).stderr)

    def test_broken_local_link_is_rejected(self):
        (self.repo / "extra.md").write_text("[bad](absent.md)\n")
        self.assertIn("Broken", self.cli("check", ok=False).stderr)

    def test_escaping_local_link_is_rejected(self):
        (self.work / "outside.md").write_text("outside")
        (self.repo / "extra.md").write_text("[bad](../outside.md)\n")
        self.cli("check", ok=False)

    def test_secret_like_token_is_rejected(self):
        (self.repo / "extra.md").write_text("ghp_" + "A" * 30)
        self.assertIn("credential", self.cli("check", ok=False).stderr)

    def test_private_contact_pattern_is_rejected(self):
        (self.repo / "extra.md").write_text("person" + "@" + "example.invalid")
        self.cli("check", ok=False)

    def test_private_path_is_rejected(self):
        private = self.repo / "private"
        private.mkdir()
        (private / "personal.md").write_text("fixture")
        self.assertIn("Private path", self.cli("check", ok=False).stderr)

    def test_gitignored_local_context_is_not_read_as_public_content(self):
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True,
                       capture_output=True, timeout=10)
        (self.repo / "PRIVATE.local.md").write_text("Private fixture, never published.\n")
        self.cli("check")

    def test_forced_tracked_private_file_is_rejected(self):
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True,
                       capture_output=True, timeout=10)
        (self.repo / "PRIVATE.local.md").write_text("Private fixture, never published.\n")
        subprocess.run(["git", "-C", str(self.repo), "add", "-f", "PRIVATE.local.md"],
                       check=True, capture_output=True, timeout=10)
        self.assertIn("Private path", self.cli("check", ok=False).stderr)

    def test_mutable_action_pin_is_rejected(self):
        path = self.repo / ".github/workflows/validate.yml"
        text = path.read_text()
        import re
        text = re.sub(r"uses: actions/checkout@[0-9a-f]{40}",
                      "uses: actions/checkout@main", text)
        path.write_text(text)
        self.assertIn("pins", self.cli("check", ok=False).stderr)

    def test_writable_ci_is_rejected(self):
        path = self.repo / ".github/workflows/validate.yml"
        path.write_text(path.read_text().replace("contents: read", "contents: write"))
        self.cli("check", ok=False)

    def test_preview_does_not_write(self):
        (self.target / "AGENTS.md").write_text("Existing project rules.\n")
        before = self.snapshot(self.target)
        result = json.loads(self.cli("adopt", self.target).stdout)
        self.assertEqual(result["mode"], "preview")
        self.assertEqual(before, self.snapshot(self.target))

    def test_install_creates_full_snapshot_and_matching_digest(self):
        self.cli("adopt", self.target, "--apply")
        guide = self.target / ".ai-guidebook/GUIDEBOOK.md"
        lock = json.loads((self.target / ".ai-guidebook/policy-lock.json").read_text())
        self.assertEqual(hashlib.sha256(guide.read_bytes()).hexdigest(),
                         lock["guidebook_sha256"])
        self.assertEqual(guide.read_bytes(), (self.repo / "GUIDEBOOK.md").read_bytes())
        for relative in ("AGENTS.md", "CLAUDE.md", "GEMINI.md", ".gitignore",
                         ".github/copilot-instructions.md", ".cursor/rules/justin-guidebook.mdc"):
            self.assertTrue((self.target / relative).is_file(), relative)

    def test_existing_project_instructions_are_preserved(self):
        original = "# Existing rules\n\nNever commit client data.\n"
        (self.target / "AGENTS.md").write_text(original)
        self.cli("adopt", self.target, "--apply")
        self.assertTrue((self.target / "AGENTS.md").read_text().startswith(original))

    def test_existing_gitignore_is_preserved(self):
        original = "build/\n*.generated\n"
        (self.target / ".gitignore").write_text(original)
        self.cli("adopt", self.target, "--apply")
        self.assertTrue((self.target / ".gitignore").read_text().startswith(original))

    def test_repeat_install_is_byte_identical(self):
        self.cli("adopt", self.target, "--apply")
        before = self.snapshot(self.target)
        result = json.loads(self.cli("adopt", self.target, "--apply").stdout)
        self.assertEqual(result["changed"], [])
        self.assertEqual(before, self.snapshot(self.target))

    def test_existing_adapter_is_never_overwritten(self):
        original = "Existing model-specific rules.\n"
        (self.target / "CLAUDE.md").write_text(original)
        result = json.loads(self.cli("adopt", self.target, "--apply").stdout)
        self.assertIn("CLAUDE.md", result["preserved_existing_adapters"])
        self.assertEqual((self.target / "CLAUDE.md").read_text(), original)

    def test_malformed_marker_refuses_all_writes(self):
        (self.target / "AGENTS.md").write_text("<!-- JUSTIN-GUIDEBOOK:BEGIN -->\n")
        before = self.snapshot(self.target)
        self.cli("adopt", self.target, "--apply", ok=False)
        self.assertEqual(before, self.snapshot(self.target))

    def test_reversed_markers_refuse_all_writes(self):
        (self.target / "AGENTS.md").write_text(
            "<!-- JUSTIN-GUIDEBOOK:END -->\n<!-- JUSTIN-GUIDEBOOK:BEGIN -->\n")
        before = self.snapshot(self.target)
        self.cli("adopt", self.target, "--apply", ok=False)
        self.assertEqual(before, self.snapshot(self.target))

    @unittest.skipIf(os.name == "nt", "Symlink permissions depend on Windows setup")
    def test_symlink_snapshot_directory_refuses_writes(self):
        outside = self.work / "outside"
        outside.mkdir()
        (self.target / ".ai-guidebook").symlink_to(outside, target_is_directory=True)
        self.cli("adopt", self.target, "--apply", ok=False)
        self.assertEqual(list(outside.iterdir()), [])
        self.assertFalse((self.target / "AGENTS.md").exists())

    @unittest.skipIf(os.name == "nt", "Symlink permissions depend on Windows setup")
    def test_symlink_adapter_refuses_all_writes(self):
        outside = self.work / "outside.md"
        outside.write_text("untouched")
        (self.target / "CLAUDE.md").symlink_to(outside)
        self.cli("adopt", self.target, "--apply", ok=False)
        self.assertEqual(outside.read_text(), "untouched")
        self.assertFalse((self.target / "AGENTS.md").exists())

    @unittest.skipIf(os.name == "nt", "Symlink permissions depend on Windows setup")
    def test_symlink_target_ancestor_is_rejected(self):
        alias = self.work / "alias"
        alias.symlink_to(self.target, target_is_directory=True)
        self.cli("adopt", alias, "--apply", ok=False)
        self.assertFalse((self.target / "AGENTS.md").exists())

    def test_tampered_existing_snapshot_is_rejected(self):
        self.cli("adopt", self.target, "--apply")
        (self.target / ".ai-guidebook/GUIDEBOOK.md").write_text("modified locally")
        before = self.snapshot(self.target)
        self.cli("adopt", self.target, "--apply", ok=False)
        self.assertEqual(before, self.snapshot(self.target))

    def test_unmanaged_snapshot_is_rejected(self):
        folder = self.target / ".ai-guidebook"
        folder.mkdir()
        (folder / "GUIDEBOOK.md").write_text("user-owned file")
        self.cli("adopt", self.target, "--apply", ok=False)
        self.assertEqual((folder / "GUIDEBOOK.md").read_text(), "user-owned file")

    def test_non_repository_target_is_rejected(self):
        outside = self.work / "not a repo"
        outside.mkdir()
        self.cli("adopt", outside, "--apply", ok=False)
        self.assertEqual(list(outside.iterdir()), [])

    def test_subdirectory_target_is_rejected(self):
        child = self.target / "child"
        child.mkdir()
        self.cli("adopt", child, "--apply", ok=False)
        self.assertEqual(list(child.iterdir()), [])

    def test_self_adoption_is_rejected(self):
        self.cli("adopt", self.repo, "--apply", ok=False)

    def test_non_directory_parent_refuses_all_writes(self):
        (self.target / ".github").write_text("not a directory")
        before = self.snapshot(self.target)
        self.cli("adopt", self.target, "--apply", ok=False)
        self.assertEqual(before, self.snapshot(self.target))

    def test_missing_target_is_not_created(self):
        target = self.work / "missing"
        self.cli("adopt", target, "--apply", ok=False)
        self.assertFalse(target.exists())

    def test_snapshot_update_preserves_unrelated_project_rules(self):
        (self.target / "AGENTS.md").write_text("Original project policy.\n")
        self.cli("adopt", self.target, "--apply")
        before = (self.target / ".ai-guidebook/GUIDEBOOK.md").read_bytes()
        self.alter_policy(policy_version="1.0.1")
        self.cli("build")
        self.cli("adopt", self.target, "--apply")
        self.assertNotEqual(before, (self.target / ".ai-guidebook/GUIDEBOOK.md").read_bytes())
        self.assertTrue((self.target / "AGENTS.md").read_text().startswith("Original project policy.\n"))

    def test_installer_does_not_commit(self):
        self.cli("adopt", self.target, "--apply")
        result = subprocess.run(["git", "-C", str(self.target), "rev-parse", "--verify", "HEAD"],
                                capture_output=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)

    def test_atomic_replace_failure_preserves_original_and_cleans_temporary(self):
        import importlib.util
        from unittest import mock
        spec = importlib.util.spec_from_file_location("fixture_guidebook", self.repo / "scripts/guidebook.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        destination = self.target / "existing.txt"
        destination.write_text("original contents")
        before = self.snapshot(self.target)
        with mock.patch.object(module.Path, "replace", side_effect=OSError("simulated I/O failure")):
            with self.assertRaises(OSError):
                module.write_atomic(destination, "replacement contents")
        self.assertEqual(before, self.snapshot(self.target))


if __name__ == "__main__":
    unittest.main()
