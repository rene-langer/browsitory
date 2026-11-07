#!/usr/bin/env python3
"""Check the user guide site for what `zensical build --strict` does not catch.

Zensical's strict mode fails the build on a link to a missing page or a missing
anchor, but (as of 0.1.0) it ignores `mkdocs.yml`'s `validation:` block, so it
does not report:

- a `nav` entry whose page does not exist,
- a page in the docs directory that the `nav` leaves out,
- an absolute link (`/install/`), which breaks under the site's
  `/browsitory/` base path,
- a relative link to a file that is not a page (a screenshot) and does not
  exist.

This script checks those four, offline, with the standard library only (no
YAML parser: it reads the simple `nav:` block form `mkdocs.yml` uses). Run by
`.github/workflows/docs.yml` next to the build; run it locally from the repo
root with `python3 scripts/check-docs-nav.py`. Exits 1 and lists every problem.
"""

import argparse
import pathlib
import re
import sys

# `- Title: page.md`, `- page.md` or `- Section:` inside the nav block.
NAV_PAGE = re.compile(r"^\s*-\s+(?:[^:]+:\s+)?([^\s:]+\.md)\s*$")
NAV_SECTION = re.compile(r"^\s*-\s+[^:]+:\s*$")
# Inline links and images: [text](target) / ![alt](target "title").
INLINE_LINK = re.compile(r"!?\[[^\]]*\]\(\s*<?([^)\s>]+)>?(?:\s+\"[^\"]*\")?\s*\)")
# Reference definitions: [id]: target
REF_DEF = re.compile(r"^\s{0,3}\[[^\]]+\]:\s+<?(\S+?)>?(?:\s|$)")
# Raw HTML attributes.
HTML_ATTR = re.compile(r"""\b(?:href|src)\s*=\s*["']([^"']+)["']""")
FENCE = re.compile(r"^\s*(```|~~~)")
INLINE_CODE = re.compile(r"`[^`]*`")
EXTERNAL = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*:")  # https:, mailto:, ...


def read_config(text: str) -> tuple[str, list[str]]:
    """Return (docs_dir, nav pages) from mkdocs.yml's text."""
    docs_dir = "docs"
    match = re.search(r"^docs_dir:\s*['\"]?([^'\"\s#]+)", text, re.MULTILINE)
    if match:
        docs_dir = match.group(1)
    pages: list[str] = []
    in_nav = False
    for line in text.splitlines():
        if re.match(r"^nav:\s*$", line):
            in_nav = True
            continue
        if not in_nav:
            continue
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if not line[0].isspace() and not line.startswith("-"):
            break  # next top-level key
        page = NAV_PAGE.match(line)
        if page:
            pages.append(page.group(1))
        elif not NAV_SECTION.match(line):
            raise ValueError(f"unrecognised nav line in mkdocs.yml: {line.strip()!r}")
    if not pages:
        raise ValueError("found no nav entries in mkdocs.yml")
    return docs_dir, pages


def link_targets(markdown: str) -> list[tuple[int, str]]:
    """Every link target in a page, with its line number; code is skipped."""
    targets = []
    in_fence = False
    for number, line in enumerate(markdown.splitlines(), start=1):
        if FENCE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        line = INLINE_CODE.sub("", line)
        for pattern in (INLINE_LINK, REF_DEF, HTML_ATTR):
            targets.extend((number, m.group(1)) for m in pattern.finditer(line))
    return targets


def check(root: pathlib.Path) -> list[str]:
    docs_dir, nav = read_config((root / "mkdocs.yml").read_text(encoding="utf-8"))
    docs = root / docs_dir
    problems = []

    for page in nav:
        if not (docs / page).is_file():
            problems.append(f"mkdocs.yml: nav entry {page} does not exist in {docs_dir}/")
    in_nav = set(nav)
    pages = sorted(p.relative_to(docs).as_posix() for p in docs.rglob("*.md"))
    for page in pages:
        if page not in in_nav:
            problems.append(f"{docs_dir}/{page}: page is not in mkdocs.yml's nav")

    for page in pages:
        source = docs / page
        for number, target in link_targets(source.read_text(encoding="utf-8")):
            where = f"{docs_dir}/{page}:{number}"
            if target.startswith("/"):
                problems.append(f"{where}: absolute link {target}; use a relative link")
                continue
            if EXTERNAL.match(target) or target.startswith("#"):
                continue
            path = target.split("#", 1)[0].split("?", 1)[0]
            if not path or path.endswith(".md") or path.endswith("/"):
                continue  # pages: zensical build --strict checks these
            if not (source.parent / path).exists():
                problems.append(f"{where}: link target {target} does not exist")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--root",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parent.parent,
        help="repository root (default: this script's repository)",
    )
    args = parser.parse_args()
    try:
        problems = check(args.root)
    except ValueError as error:
        print(f"check-docs-nav: {error}", file=sys.stderr)
        return 1
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        print(f"check-docs-nav: {len(problems)} problem(s)", file=sys.stderr)
        return 1
    print("check-docs-nav: nav, pages and links OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
