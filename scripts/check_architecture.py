#!/usr/bin/env python3
"""Enforce bounded Rust modules and a shrinking allowance for existing debt."""
from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BUDGET = Path("scripts/architecture_budget.json")
SOURCE_LIMIT = 800
TEST_LIMIT = 1000
# Diesel owns this file. Exemptions are deliberately not configurable in the budget.
GENERATED_SCHEMA = "db-sqlite/src/schema.rs"


def limit_for(path: str, exceptions: dict[str, int]) -> int:
    default = TEST_LIMIT if "/tests/" in path or path.endswith("_tests.rs") else SOURCE_LIMIT
    return exceptions.get(path, default)


def violations(files: dict[str, str], exceptions: dict[str, int]) -> list[str]:
    errors = []
    for path, source in sorted(files.items()):
        count = len(source.splitlines())
        limit = limit_for(path, exceptions)
        if count > limit:
            errors.append(f"{path}: {count} lines exceeds {limit}; split by responsibility")
    for path in sorted(exceptions):
        if path not in files:
            errors.append(f"{path}: remove the obsolete size allowance")
    return errors


def ratchet(previous: dict[str, int], current: dict[str, int]) -> list[str]:
    return [f"{path}: existing size allowances may only shrink"
            for path, limit in sorted(current.items())
            if path not in previous or limit > previous[path]]


def source_files(root: Path) -> dict[str, str]:
    # Include untracked source during development, excluding build output and vendored code.
    return {str(path.relative_to(root)): path.read_text()
            for crate in root.iterdir() if (crate / "Cargo.toml").is_file()
            for directory in (crate / "src", crate / "tests") if directory.is_dir()
            for path in directory.rglob("*.rs")
            if str(path.relative_to(root)) != GENERATED_SCHEMA}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--base", help="Git revision to check for increased debt allowances")
    args = parser.parse_args()
    root = args.root.resolve()
    config = json.loads((root / BUDGET).read_text())
    exceptions = config["existing_files"]
    files = source_files(root)
    errors = violations(files, exceptions)
    if args.base:
        old = subprocess.run(["git", "show", f"{args.base}:{BUDGET.as_posix()}"],
                             cwd=root, text=True, capture_output=True, check=False)
        if old.returncode == 0:
            errors.extend(ratchet(json.loads(old.stdout)["existing_files"], exceptions))
        else:
            # Only the first introduction may lack a budget. Invalid revisions must fail.
            subprocess.run(["git", "rev-parse", "--verify", args.base], cwd=root,
                           check=True, stdout=subprocess.DEVNULL)
    if errors:
        print("\n".join(errors))
        return 1
    print(f"Architecture size gate: {len(files)} Rust files checked; {len(exceptions)} existing allowances.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
