#!/usr/bin/env python3
"""Fail if code changed without a matching CHANGELOG.md update.

Used both as a local `pre-push` hook (scripts/hooks/pre-push) and as a CI
job (.github/workflows/ci.yml's `changelog` job), so the rule lives here
once and both callers just invoke this script.
"""

import argparse
import os
import subprocess
import sys

WATCHED_PREFIXES = ("crates/", "frontend/src/", "e2e/")
CHANGELOG_FILE = "CHANGELOG.md"


def needs_changelog(changed_files: list[str]) -> bool:
    """True if changed_files touch a watched path without touching CHANGELOG.md."""
    touches_watched_path = any(f.startswith(WATCHED_PREFIXES) for f in changed_files)
    touches_changelog = CHANGELOG_FILE in changed_files
    return touches_watched_path and not touches_changelog


def changed_files(base: str) -> list[str]:
    merge_base = subprocess.run(
        ["git", "merge-base", "HEAD", base],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    diff = subprocess.run(
        ["git", "diff", "--name-only", merge_base, "HEAD"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [line for line in diff.splitlines() if line]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--base",
        default="main",
        help="ref to diff against (default: main)",
    )
    args = parser.parse_args()

    if os.environ.get("SKIP_CHANGELOG_CHECK") == "1":
        print("check-changelog: skipped via SKIP_CHANGELOG_CHECK=1")
        return 0

    files = changed_files(args.base)
    if needs_changelog(files):
        print(
            "check-changelog: code changed under crates/, frontend/src/, or "
            "e2e/ but CHANGELOG.md wasn't updated.\n"
            "Add an entry under [Unreleased], or bypass with "
            "SKIP_CHANGELOG_CHECK=1 (local push) / the 'skip-changelog' PR "
            "label (CI).",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
