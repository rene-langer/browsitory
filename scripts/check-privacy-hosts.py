#!/usr/bin/env python3
"""Keeps the privacy notice (docs/guide/privacy.md) in step with the code (#627).

Lists every host named in an `http://`, `https://`, `ws://` or `wss://` URL in the app's
non-test sources and the desktop app's Tauri config (the updater endpoints), and fails when:

- a host is neither on ALLOWED_HOSTS nor a local or reserved example name (IGNORED_HOSTS,
  `*.test`, `*.example`, `*.invalid`, `*.localhost`), so a new outbound host can't slip in
  without the notice saying so;
- a host on ALLOWED_HOSTS is not named, in backticks, in docs/guide/privacy.md;
- a host on ALLOWED_HOSTS no longer appears in the sources (a stale entry).

Scanned: `crates/*/src/**/*.rs` (except the test-support crate), `frontend/src` and
`extension/src` (`.ts`, `.tsx`, `.js`, `.mjs`, `.css`, `.html`), and
`crates/tauri-app/tauri.conf.json`. Skipped: comments, test files (`*.test.*`, `*.spec.*`,
`tests.rs`, `tests/`, `__tests__/`, `fixtures/`) and Rust `#[cfg(test)] mod … { … }` blocks.

It only finds hosts written into the code. Hosts that come from the user (Git remotes,
self-hosted forge URLs) are documented in the notice by hand.

Usage: python3 scripts/check-privacy-hosts.py [--root DIR]
CI: the `audit` job in .github/workflows/ci.yml.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# Each host the app contacts on its own, and why. docs/guide/privacy.md must name each one.
ALLOWED_HOSTS = {
    "github.com": "update check and download (tauri.conf.json updater endpoint)",
    "rene-langer.github.io": "update check fallback, and the user guide link",
    "api.github.com": "pull requests for GitHub remotes",
    "api.bitbucket.org": "pull requests for Bitbucket remotes",
}

# Local addresses and documentation names: never a contact with a third party.
IGNORED_HOSTS = {"localhost", "127.0.0.1", "[::1]", "example.com", "example.net", "example.org"}
IGNORED_SUFFIXES = (".test", ".example", ".invalid", ".localhost", ".example.com", ".example.net",
                    ".example.org")

URL_RE = re.compile(r"\b(?:https?|wss?)://(\[[0-9A-Fa-f:]+\]|[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*)")
# `//` that starts a comment, not the one in `scheme://`.
LINE_COMMENT_RE = re.compile(r"(?<![:/])//.*$")
SOURCE_SUFFIXES = {".rs", ".ts", ".tsx", ".js", ".mjs", ".css", ".html"}
TEST_DIRS = {"tests", "__tests__", "fixtures", "test-support"}


def is_test_file(path: Path) -> bool:
    name = path.name
    return (
        ".test." in name
        or ".spec." in name
        or name == "tests.rs"
        or any(part in TEST_DIRS for part in path.parts)
    )


def strip_rust_test_modules(lines: list[str]) -> list[str]:
    """Drops each `#[cfg(test)]`-gated `mod name {` block (rustfmt puts its closing brace at the
    `mod` line's indentation). Other `#[cfg(test)]` items are short helpers and stay in."""
    kept: list[str] = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if line.strip() == "#[cfg(test)]" and i + 1 < len(lines):
            header = lines[i + 1]
            match = re.match(r"^(\s*)(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{\s*$", header)
            if match:
                closing = match.group(1) + "}"
                j = i + 2
                while j < len(lines) and lines[j].rstrip() != closing:
                    j += 1
                i = j + 1
                continue
        kept.append(line)
        i += 1
    return kept


def code_lines(path: Path, text: str) -> list[str]:
    """The lines of `text` without comments (and, for Rust, without test modules)."""
    lines = text.splitlines()
    if path.suffix == ".rs":
        lines = strip_rust_test_modules(lines)
    out: list[str] = []
    in_block = False
    for line in lines:
        if in_block:
            if "*/" in line:
                in_block = False
                line = line.split("*/", 1)[1]
            else:
                continue
        stripped = line.lstrip()
        if stripped.startswith("/*"):
            if "*/" not in stripped:
                in_block = True
            continue
        if stripped.startswith("*") or stripped.startswith("<!--"):
            continue
        out.append(LINE_COMMENT_RE.sub("", line))
    return out


def is_ignored(host: str) -> bool:
    return host in IGNORED_HOSTS or host.endswith(IGNORED_SUFFIXES)


def source_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for src in sorted((root / "crates").glob("*/src")):
        files.extend(src.rglob("*.rs"))
    for folder in ("frontend/src", "extension/src"):
        base = root / folder
        if base.is_dir():
            files.extend(p for p in base.rglob("*") if p.suffix in SOURCE_SUFFIXES)
    rel = [p for p in files if not is_test_file(p.relative_to(root))]
    conf = root / "crates/tauri-app/tauri.conf.json"
    if conf.is_file():
        rel.append(conf)
    return sorted(rel)


def find_hosts(root: Path) -> dict[str, list[str]]:
    """Host -> the files that name it."""
    found: dict[str, list[str]] = {}
    for path in source_files(root):
        text = path.read_text(encoding="utf-8", errors="replace")
        lines = text.splitlines() if path.suffix == ".json" else code_lines(path, text)
        for line in lines:
            for match in URL_RE.finditer(line):
                host = match.group(1).lower()
                if not is_ignored(host):
                    found.setdefault(host, []).append(path.relative_to(root).as_posix())
    return found


def check(root: Path) -> list[str]:
    errors: list[str] = []
    found = find_hosts(root)
    notice_path = root / "docs/guide/privacy.md"
    notice = notice_path.read_text(encoding="utf-8") if notice_path.is_file() else ""
    for host, places in sorted(found.items()):
        if host not in ALLOWED_HOSTS:
            errors.append(
                f"{host} (in {', '.join(sorted(set(places)))}) is not on ALLOWED_HOSTS: document "
                "the contact in docs/guide/privacy.md and add it to scripts/check-privacy-hosts.py"
            )
    for host in sorted(ALLOWED_HOSTS):
        if f"`{host}`" not in notice:
            errors.append(f"{host} is on ALLOWED_HOSTS but docs/guide/privacy.md doesn't name `{host}`")
        if host not in found:
            errors.append(f"{host} is on ALLOWED_HOSTS but no source names it any more: remove it "
                          "(and update docs/guide/privacy.md)")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args()
    errors = check(args.root)
    for error in errors:
        print(f"error: {error}", file=sys.stderr)
    if errors:
        return 1
    print(f"Privacy notice covers every hard-coded host: {', '.join(sorted(ALLOWED_HOSTS))}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
