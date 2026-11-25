#!/usr/bin/env python3
"""Check that a release tag may be released (release.yml's `verify` job, #1111).

Fails, listing every problem, unless:

- the tag is `release/MAJOR.MINOR.PATCH` (no leading zeros, no suffix);
- CHANGELOG.md at the tagged commit has an empty `## [Unreleased]` followed directly by
  `## [X.Y.Z] - YYYY-MM-DD` for the tag's version, with entries under it (docs/RELEASING.md,
  "Preparing the changelog"). The app's release notes and every VSIX's changelog come from
  that section;
- the commit is on main (an ancestor of `--main-ref`, default `origin/main`). Needs the full
  history: a shallow clone is refused;
- a ci.yml run on main succeeded for the commit (`gh run list`; needs GH_TOKEN with
  `actions: read` in a workflow). A run still in progress fails too: re-run the job once it
  has passed.

Run it by hand before tagging, from an up-to-date clone:

    python3 scripts/check-release-tag.py --tag release/X.Y.Z --commit origin/main

Python standard library only.
"""

import argparse
import datetime
import json
import pathlib
import re
import subprocess
import sys

TAG = re.compile(r"^release/((?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*))$")
UNRELEASED = re.compile(r"^##\s+\[Unreleased\]\s*$", re.IGNORECASE)
SECTION = re.compile(r"^##\s")
RELEASE_HEADING = re.compile(r"^## \[([^\]]+)\] - (\S+)\s*$")
CI_WORKFLOW = "ci.yml"
MAIN_BRANCH = "main"


class CheckError(Exception):
    """The check couldn't run (git or gh failed, shallow clone)."""


def parse_tag(tag: str) -> str | None:
    """The version of a `release/X.Y.Z` tag, or None for any other name."""
    match = TAG.match(tag)
    return match.group(1) if match else None


def _is_date(value: str) -> bool:
    # fromisoformat alone would also accept 20261130.
    if re.fullmatch(r"\d{4}-\d{2}-\d{2}", value) is None:
        return False
    try:
        datetime.date.fromisoformat(value)
    except ValueError:
        return False
    return True


def changelog_problems(text: str, version: str) -> list[str]:
    """What keeps CHANGELOG.md text from being the changelog of release `version`."""
    lines = text.replace("\r\n", "\n").split("\n")
    start = next((i for i, line in enumerate(lines) if UNRELEASED.match(line)), None)
    if start is None:
        return [
            "CHANGELOG.md has no '## [Unreleased]' heading; docs/RELEASING.md's release prep "
            f"renames it to '## [{version}] - YYYY-MM-DD' and adds an empty one above"
        ]
    problems = []
    nxt = next((i for i in range(start + 1, len(lines)) if SECTION.match(lines[i])), None)
    if any(line.strip() for line in lines[start + 1 : nxt]):
        problems.append(
            "CHANGELOG.md's '## [Unreleased]' is not empty: those entries would ship labelled "
            f"Unreleased; move them into '## [{version}] - YYYY-MM-DD'"
        )
    heading = lines[nxt] if nxt is not None else None
    match = RELEASE_HEADING.match(heading) if heading is not None else None
    if match is None or match.group(1) != version:
        found = f"'{heading}'" if heading is not None else "nothing"
        problems.append(
            f"CHANGELOG.md has no '## [{version}] - YYYY-MM-DD' section as the first one after "
            f"'## [Unreleased]' (found {found})"
        )
        return problems
    if not _is_date(match.group(2)):
        problems.append(f"CHANGELOG.md's '{heading}' has no valid YYYY-MM-DD date")
    end = next((i for i in range(nxt + 1, len(lines)) if SECTION.match(lines[i])), len(lines))
    if not any(line.strip() for line in lines[nxt + 1 : end]):
        problems.append(f"CHANGELOG.md's '{heading}' section is empty")
    return problems


def ci_problems(runs: list[dict]) -> list[str]:
    """What keeps these ci.yml runs of one commit from showing that main's CI passed for it."""
    on_main = [run for run in runs if run.get("headBranch") == MAIN_BRANCH]
    if any(run.get("conclusion") == "success" for run in on_main):
        return []
    if any(run.get("status") != "completed" for run in on_main):
        return [
            f"{CI_WORKFLOW} on {MAIN_BRANCH} is still running for this commit; re-run this "
            "check once it has passed"
        ]
    if on_main:
        conclusions = ", ".join(sorted({str(run.get("conclusion")) for run in on_main}))
        return [f"{CI_WORKFLOW} on {MAIN_BRANCH} did not pass for this commit ({conclusions})"]
    return [f"{CI_WORKFLOW} never ran on {MAIN_BRANCH} for this commit"]


def _git(root: pathlib.Path, *args: str) -> subprocess.CompletedProcess:
    try:
        return subprocess.run(["git", *args], cwd=root, capture_output=True, text=True)
    except OSError as error:
        raise CheckError(f"git could not run: {error}") from error


def is_on_main(root: pathlib.Path, commit: str, main_ref: str) -> bool:
    """True if commit is an ancestor of (or equal to) main_ref."""
    shallow = _git(root, "rev-parse", "--is-shallow-repository")
    if shallow.returncode != 0:
        raise CheckError(f"git rev-parse failed: {shallow.stderr.strip()}")
    if shallow.stdout.strip() == "true":
        raise CheckError(
            "this is a shallow clone, so whether the commit is on main can't be told; "
            "check out with fetch-depth: 0 (or run `git fetch --unshallow`)"
        )
    if _git(root, "rev-parse", "--verify", "--quiet", f"{main_ref}^{{commit}}").returncode != 0:
        raise CheckError(f"{main_ref} not found; fetch main first")
    result = _git(root, "merge-base", "--is-ancestor", commit, main_ref)
    if result.returncode in (0, 1):
        return result.returncode == 0
    raise CheckError(f"git merge-base failed: {result.stderr.strip()}")


def gh_ci_runs(sha: str, repo: str | None) -> list[dict]:
    """ci.yml's runs for commit sha, newest first, as `gh run list --json` gives them."""
    cmd = [
        "gh", "run", "list", "--workflow", CI_WORKFLOW, "--commit", sha, "--limit", "100",
        "--json", "headBranch,event,status,conclusion",
    ]
    if repo:
        cmd += ["--repo", repo]
    try:
        result = subprocess.run(cmd, capture_output=True, text=True)
    except OSError as error:
        raise CheckError(f"gh could not run: {error}") from error
    if result.returncode != 0:
        raise CheckError(f"gh run list failed: {result.stderr.strip()}")
    try:
        runs = json.loads(result.stdout or "[]")
    except ValueError as error:
        raise CheckError(f"gh run list returned bad JSON: {error}") from error
    if not isinstance(runs, list):
        raise CheckError("gh run list returned no list")
    return runs


def main(argv: list[str] | None = None, list_ci_runs=gh_ci_runs) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--tag", required=True, help="the tag name, release/X.Y.Z")
    parser.add_argument("--commit", default="HEAD", help="the tagged commit (default: HEAD)")
    parser.add_argument("--main-ref", default=f"origin/{MAIN_BRANCH}", help="default: origin/main")
    parser.add_argument("--repo", help="OWNER/NAME for gh (default: the clone's GitHub remote)")
    parser.add_argument(
        "--root",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parent.parent,
        help="repository root (default: this script's repository)",
    )
    args = parser.parse_args(argv)

    version = parse_tag(args.tag)
    if version is None:
        print(
            f"check-release-tag: tag '{args.tag}' is not release/MAJOR.MINOR.PATCH",
            file=sys.stderr,
        )
        return 1

    problems: list[str] = []
    try:
        resolved = _git(args.root, "rev-parse", "--verify", "--quiet", f"{args.commit}^{{commit}}")
        if resolved.returncode != 0:
            raise CheckError(f"commit '{args.commit}' not found")
        sha = resolved.stdout.strip()

        shown = _git(args.root, "show", f"{sha}:CHANGELOG.md")
        if shown.returncode != 0:
            problems.append(f"no CHANGELOG.md at {sha}")
        else:
            problems += changelog_problems(shown.stdout, version)

        if not is_on_main(args.root, sha, args.main_ref):
            problems.append(
                f"commit {sha} is not on {MAIN_BRANCH} ({args.main_ref}); tag a commit that "
                "went through main's CI"
            )

        problems += ci_problems(list_ci_runs(sha, args.repo))
    except CheckError as error:
        print(f"check-release-tag: {error}", file=sys.stderr)
        return 1

    if problems:
        print(f"check-release-tag: {args.tag} can't be released:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        print("See docs/RELEASING.md.", file=sys.stderr)
        return 1
    print(f"check-release-tag: {args.tag} ({sha}) passes.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
