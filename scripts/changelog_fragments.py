"""Changelog fragments: one file per pull request under changelog.d/.

Shared by check-changelog.py (a PR adds a fragment) and fold-changelog.py (a
release folds all fragments into CHANGELOG.md). The format is described in
changelog.d/README.md.
"""

import posixpath

FRAGMENT_DIR = "changelog.d"
FRAGMENT_SUFFIX = ".md"
README = "README.md"


def is_fragment(path: str) -> bool:
    """True for changelog.d/<name>.md, except changelog.d/README.md.

    Only files directly in changelog.d/ count; subfolders and other file types
    don't.
    """
    folder, name = posixpath.split(path)
    return (
        folder == FRAGMENT_DIR
        and name.endswith(FRAGMENT_SUFFIX)
        and name != README
        and name != FRAGMENT_SUFFIX
    )


def fragment_problems(text: str) -> list[str]:
    """What's wrong with a fragment's content; an empty list if nothing.

    A fragment is one section of [Unreleased]: a `### Heading` line first, then
    at least one more line (intro and/or bullets). It must not contain `#` or
    `##` headings (outside code fences), which would break CHANGELOG.md's
    structure once folded in.
    """
    lines = text.splitlines()
    content = [line for line in lines if line.strip()]
    if not content:
        return ["is empty"]
    problems = []
    first = content[0]
    if not first.startswith("### ") or not first[4:].strip():
        problems.append("must start with a '### Heading' line")
    elif len(content) < 2:
        problems.append("has a heading but no entries")
    in_fence = False
    for line in lines:
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if not in_fence and (line.startswith("# ") or line.startswith("## ")):
            problems.append(
                f"must not contain a '#' or '##' heading: {line.strip()!r}"
            )
    return problems
