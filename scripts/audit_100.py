#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Re-run the full repository validation and regression suite 100 independent times.

This is repetition/stability evidence, not 100 independent semantic or legal audits.
Report real exit codes and observed unittest counts rather than inferring success.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
TEST_PATTERN = re.compile(r"Ran (\d+) tests? in")
EXPECTED_ROUNDS = 100


def run_suite(round_number):
    env = os.environ.copy()
    env["PYTHONHASHSEED"] = str(round_number)
    checks = [
        [sys.executable, str(ROOT / "scripts/guidebook.py"), "check"],
        [sys.executable, "-m", "unittest", "discover", "-s",
         str(ROOT / "tests"), "-p", "test*.py", "-v"],
        [os.environ.get("GUIDEBOOK_RUST_BINARY", str(ROOT / "rust/audit/target/debug/guidebook-audit")), "check", str(ROOT)],
        ["cargo", "+1.85.1", "test", "--locked", "--offline",
         "--manifest-path", str(ROOT / "rust/audit/Cargo.toml")],
    ]
    record = {"round": round_number, "hash_seed": round_number, "steps": []}
    for cmd in checks:
        command_name = ("check" if cmd[-1] == "check" else
                        "unittest" if "-m" in cmd else
                        "rust_tests" if "--manifest-path" in cmd else "rust")
        try:
            proc = subprocess.run(
                cmd, cwd=ROOT.parent, capture_output=True, text=True,
                timeout=90, env=env, check=False)
            result = {"command": command_name, "exit_code": proc.returncode,
                      "passed": proc.returncode == 0}
            if result["command"] == "unittest":
                match = TEST_PATTERN.search(proc.stderr + "\n" + proc.stdout)
                result["tests_executed"] = int(match.group(1)) if match else 0
                result["passed"] &= "OK" in proc.stderr and result["tests_executed"] > 0
            if command_name == "rust_tests":
                match = re.search(r"test result: ok\. (\d+) passed;", proc.stdout)
                result["tests_executed"] = int(match.group(1)) if match else 0
                result["passed"] &= result["tests_executed"] > 0
            if not result["passed"]:
                result["output_tail"] = (proc.stdout + "\n" + proc.stderr)[-2500:]
        except subprocess.TimeoutExpired:
            result = {"command": command_name,
                      "exit_code": None, "passed": False, "timeout": True}
        record["steps"].append(result)
    record["passed"] = all(step["passed"] for step in record["steps"])
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rounds", type=int, default=EXPECTED_ROUNDS)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()
    if args.rounds < 1 or args.rounds > 100:
        parser.error("Rounds must be between 1 and 100")
    started = time.monotonic()
    records = []
    for i in range(args.rounds):
        record = run_suite(i)
        records.append(record)
        print("Audit pass {}/{}: {}".format(i + 1, args.rounds,
              "passed" if record["passed"] else "FAILED"), flush=True)
        if not record["passed"]:
            for step in record["steps"]:
                if not step["passed"]:
                    print(json.dumps(step, sort_keys=True), flush=True)
    current_commit = subprocess.run(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"],
        capture_output=True, text=True, check=False, timeout=10)
    summary = {
        "commit_sha": current_commit.stdout.strip() if current_commit.returncode == 0 else None,
        "requested_rounds": args.rounds,
        "completed_rounds": len(records),
        "successful_rounds": sum(int(record["passed"]) for record in records),
        "validation_checks_passed": sum(int(step["passed"]) for record in records
                                        for step in record["steps"] if step["command"] == "check"),
        "unittest_suites_passed": sum(int(step["passed"]) for record in records
                                     for step in record["steps"] if step["command"] == "unittest"),
        "unittest_cases_executed": sum(step.get("tests_executed", 0) for record in records
                                      for step in record["steps"]),
        "rust_audits_passed": sum(int(step["passed"]) for record in records
                                 for step in record["steps"] if step["command"] == "rust"),
        "rust_unittest_suites_passed": sum(int(step["passed"]) for record in records
                                         for step in record["steps"] if step["command"] == "rust_tests"),
        "rust_unittest_cases_executed": sum(step.get("tests_executed", 0)
                                            for record in records for step in record["steps"]
                                            if step["command"] == "rust_tests"),
        "elapsed_seconds": round(time.monotonic() - started, 3),
        "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "rounds": records,
    }
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print("AUDIT_100_SUMMARY: " + json.dumps(
        {k: v for k, v in summary.items() if k != "rounds"}, sort_keys=True), flush=True)
    return 0 if summary["successful_rounds"] == args.rounds else 1


if __name__ == "__main__":
    sys.exit(main())
