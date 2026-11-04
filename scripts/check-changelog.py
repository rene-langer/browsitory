#!/usr/bin/env python3
"""Fail if code changed without a changelog entry.

An entry is a new or changed fragment under changelog.d/ (see
changelog.d/README.md) or, as before, a change to CHANGELOG.md itself.
Fragments the change adds or edits must be well-formed.

Used both as a local `pre-push` hook (scripts/hooks/pre-push) and as a CI
job (.github/workflows/ci.yml's `changelog` job), so the rule lives here
once and both callers just invoke this script.
"""

import argparse
import os
import posixpath
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from changelog_fragments import fragment_problems, is_fragment  # noqa: E402

WATCHED_PREFIXES = ("crates/", "frontend/src/", "e2e/")
CHANGELOG_FILE = "CHANGELOG.md"

# Dependency manifests and lockfiles, matched by file name in any directory. A
# change touching only these (e.g. a Renovate PR bumping `crates/*/Cargo.toml` or
# `e2e/pnpm-lock.yaml`) doesn't need a changelog entry.
DEPENDENCY_FILES = frozenset(
    {
        "Cargo.toml",
        "Cargo.lock",
        "package.json",
        "pnpm-lock.yaml",
        "pnpm-workspace.yaml",
        "rust-toolchain.toml",
    }
)


def is_dependency_file(path: str) -> bool:
    return posixpath.basename(path) in DEPENDENCY_FILES


def fragment_files(changed_files: list[str]) -> list[str]:
    """The changelog fragments among changed_files."""
    return [f for f in changed_files if is_fragment(f)]


def needs_changelog(changed_files: list[str]) -> bool:
    """True if changed_files touch a watched path without a changelog entry.

    A changelog entry is CHANGELOG.md or a fragment (changelog.d/*.md other than
    README.md). Dependency manifests/lockfiles (DEPENDENCY_FILES) don't count as
    touching a watched path; any other file under one still needs an entry.
    """
    touches_watched_path = any(
        f.startswith(WATCHED_PREFIXES) and not is_dependency_file(f)
        for f in changed_files
    )
    has_entry = CHANGELOG_FILE in changed_files or bool(fragment_files(changed_files))
    return touches_watched_path and not has_entry


def parse_name_status(output: str) -> list[str]:
    """Paths from `git diff --name-status --no-renames` output.

    A deleted fragment is dropped: removing one (as fold-changelog.py does) is
    not an entry. Other deletions stay, so deleting code still needs an entry.
    """
    files = []
    for line in output.splitlines():
        if not line:
            continue
        status, path = line.split("\t", 1)
        if status.startswith("D") and is_fragment(path):
            continue
        files.append(path)
    return files


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, check=True
    ).stdout


def changed_files(base: str) -> list[str]:
    merge_base = git("merge-base", "HEAD", base).strip()
    return parse_name_status(
        git("diff", "--name-status", "--no-renames", merge_base, "HEAD")
    )


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

    bad = False
    for path in fragment_files(files):
        for problem in fragment_problems(git("show", f"HEAD:{path}")):
            print(f"check-changelog: {path} {problem}.", file=sys.stderr)
            bad = True
    if bad:
        print(
            "See changelog.d/README.md for the fragment format.", file=sys.stderr
        )
        return 1

    if needs_changelog(files):
        print(
            "check-changelog: code changed under crates/, frontend/src/, or "
            "e2e/ but there is no changelog entry.\n"
            "(Changes limited to dependency manifests/lockfiles are exempt.)\n"
            "Add a fragment, changelog.d/<issue>-<short-slug>.md (see "
            "changelog.d/README.md), or bypass with SKIP_CHANGELOG_CHECK=1 "
            "(local push) / the 'skip-changelog' PR label (CI).",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
