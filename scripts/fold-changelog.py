#!/usr/bin/env python3
"""Fold changelog fragments (changelog.d/*.md) into CHANGELOG.md.

Each fragment becomes a section at the top of `## [Unreleased]`, and the
fragment files are deleted. Run it in a release-prep pull request, before
renaming [Unreleased] to the new version (see docs/RELEASING.md).

Order: newest first, by the date of the commit that added each fragment
(`git log --diff-filter=A`). Fragments with no such commit (not committed yet,
or no git history) come first. Ties go by file name.

Safe to re-run: with no fragments left it changes nothing, and a fragment whose
text is already in [Unreleased] is not added again. It refuses, changing
nothing, if CHANGELOG.md has no `## [Unreleased]` heading or a fragment is
malformed.

--check changes nothing and exits 1 while fragments remain; the release
workflow uses it so a release never ships a CHANGELOG.md without them.

Python standard library only.
"""

import argparse
import os
import pathlib
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from changelog_fragments import (  # noqa: E402
    FRAGMENT_DIR,
    fragment_problems,
    is_fragment,
)

CHANGELOG_FILE = "CHANGELOG.md"
UNRELEASED = re.compile(r"^##\s+\[Unreleased\]\s*$", re.IGNORECASE)
NEXT_SECTION = re.compile(r"^##\s")


def _strip_blank_edges(lines: list[str]) -> list[str]:
    start, end = 0, len(lines)
    while start < end and not lines[start].strip():
        start += 1
    while end > start and not lines[end - 1].strip():
        end -= 1
    return lines[start:end]


def fold_text(changelog: str, fragments: list[str]) -> str:
    """CHANGELOG.md text with fragments inserted, in order, at the top of [Unreleased].

    Raises ValueError if there is no `## [Unreleased]` heading. A fragment whose
    text is already in [Unreleased] is skipped; with nothing to add, the text is
    returned unchanged.
    """
    lines = changelog.split("\n")
    start = next((i for i, line in enumerate(lines) if UNRELEASED.match(line)), None)
    if start is None:
        raise ValueError(f"{CHANGELOG_FILE} has no '## [Unreleased]' heading")
    end = next(
        (i for i in range(start + 1, len(lines)) if NEXT_SECTION.match(lines[i])),
        len(lines),
    )
    body = "\n".join(_strip_blank_edges(lines[start + 1 : end]))

    blocks = []
    for fragment in fragments:
        block = "\n".join(_strip_blank_edges(fragment.split("\n")))
        if block and block not in body and block not in blocks:
            blocks.append(block)
    if not blocks:
        return changelog
    if body:
        blocks.append(body)

    head = lines[: start + 1]
    tail = lines[end:]
    if tail:
        return "\n".join([*head, "", "\n\n".join(blocks), "", *tail])
    return "\n".join([*head, "", "\n\n".join(blocks)]) + "\n"


def order_fragments(added_at: dict[str, int | None]) -> list[str]:
    """Fragment names, newest first.

    added_at maps a name to the Unix time of the commit that added it, or None
    if there is none. None comes first; ties go by name.
    """

    def key(name: str):
        timestamp = added_at[name]
        if timestamp is None:
            return (0, 0, name)
        return (1, -timestamp, name)

    return sorted(added_at, key=key)


def commit_time_of_add(root: pathlib.Path, path: str) -> int | None:
    """Unix time of the latest commit that added path, or None."""
    try:
        result = subprocess.run(
            ["git", "log", "--diff-filter=A", "--format=%ct", "-1", "--", path],
            cwd=root,
            capture_output=True,
            text=True,
        )
    except OSError:
        return None
    output = result.stdout.strip()
    if result.returncode != 0 or not output:
        return None
    return int(output.splitlines()[0])


def find_fragments(root: pathlib.Path) -> list[str]:
    """Paths (relative to root) of all fragments, sorted by name."""
    folder = root / FRAGMENT_DIR
    if not folder.is_dir():
        return []
    return sorted(
        f"{FRAGMENT_DIR}/{entry.name}"
        for entry in folder.iterdir()
        if entry.is_file() and is_fragment(f"{FRAGMENT_DIR}/{entry.name}")
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="change nothing; exit 1 if any fragments are left to fold",
    )
    parser.add_argument(
        "--root",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parent.parent,
        help="repository root (default: this script's repository)",
    )
    args = parser.parse_args(argv)
    root = args.root

    fragments = find_fragments(root)
    if args.check:
        if fragments:
            print(
                "fold-changelog: these changelog fragments are not in "
                f"{CHANGELOG_FILE} yet:",
                file=sys.stderr,
            )
            for path in fragments:
                print(f"  {path}", file=sys.stderr)
            print(
                "Run scripts/fold-changelog.py and commit the result "
                "(see docs/RELEASING.md).",
                file=sys.stderr,
            )
            return 1
        print("fold-changelog: no fragments left to fold.")
        return 0

    if not fragments:
        print("fold-changelog: no fragments to fold.")
        return 0

    texts = {path: (root / path).read_text(encoding="utf-8") for path in fragments}
    bad = False
    for path, text in texts.items():
        for problem in fragment_problems(text):
            print(f"fold-changelog: {path} {problem}.", file=sys.stderr)
            bad = True
    if bad:
        print("Nothing changed. See changelog.d/README.md.", file=sys.stderr)
        return 1

    ordered = order_fragments({path: commit_time_of_add(root, path) for path in fragments})
    changelog_path = root / CHANGELOG_FILE
    try:
        folded = fold_text(
            changelog_path.read_text(encoding="utf-8"),
            [texts[path] for path in ordered],
        )
    except (OSError, ValueError) as error:
        print(f"fold-changelog: {error}. Nothing changed.", file=sys.stderr)
        return 1

    changelog_path.write_text(folded, encoding="utf-8")
    for path in ordered:
        (root / path).unlink()
        print(f"fold-changelog: folded {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
