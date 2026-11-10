#!/usr/bin/env python3
"""Keeps the privacy notice (docs/guide/privacy.md) in step with the code (#627).

Lists every host named in an `http://`, `https://`, `ws://` or `wss://` URL (scheme in any
case) or a protocol-relative `//host.name` URL in the app's non-test sources, and fails when:

- a host is neither on ALLOWED_HOSTS, nor on NON_NETWORK_HOSTS, nor a local or reserved example
  name (IGNORED_HOSTS, `*.test`, `*.example`, `*.invalid`, `*.localhost`), so a new outbound
  host can't slip in without the notice saying so;
- a URL's host isn't written out after the scheme (`format!("https://{HOST}")`,
  `concat!("https://", …)`, `` `https://${host}` ``, `"https://" + host`) and the line isn't on
  DYNAMIC_URLS, so a host kept in a constant elsewhere can't slip in either;
- a host on ALLOWED_HOSTS is not named, in backticks, in docs/guide/privacy.md;
- a host on ALLOWED_HOSTS, or a DYNAMIC_URLS entry, no longer appears in the sources (stale).

The scan is deliberately simple (#706): it reads the whole text, comments included, and
doesn't parse strings or comments. A URL in a comment is a cheap false positive: add it to
NON_NETWORK_HOSTS with the reason.

Scanned: `crates/*/src/**/*.rs` (except the test-support crate), `frontend/src` and
`extension/src` (`.ts`, `.tsx`, `.js`, `.mjs`, `.css`, `.html`, `.json`, `.svg`),
`frontend/index.html`, `frontend/public/`, `frontend/vite*.ts`, `extension/package.json`,
`crates/tauri-app/tauri.conf.json` and `crates/tauri-app/capabilities/`. Skipped: test files
(`*.test.*`, `*.spec.*`, `tests.rs`, `tests/`, `__tests__/`, `fixtures/`), Rust
`#[cfg(test)] mod … { … }` blocks and the files of `#[cfg(test)] mod name;` modules.

It only finds hosts written into the code. Hosts that come from the user (Git remotes,
self-hosted forge URLs) are documented in the notice by hand.

Usage: python3 scripts/check-privacy-hosts.py [--root DIR]
CI: the `audit` job in .github/workflows/ci.yml (it runs even when an audit step fails, #707).
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

# Hosts the sources name that the app never contacts, and why.
NON_NETWORK_HOSTS = {
    "www.w3.org": "XML namespace in SVG files",
    "vite.dev": "documentation link in a comment (frontend/vite.config.ts)",
    "vitest.dev": "documentation link in a comment (frontend/src/jest-dom-vitest.d.ts)",
}

# Lines whose URL has no literal host, by file: each entry is a snippet of the line, and why the
# host isn't a contact the app makes on its own.
DYNAMIC_URLS = {
    "crates/repo-service/src/clone.rs": {
        "over `https://` with": "comment naming the scheme",
        'eq_ignore_ascii_case("https://")': "compares a remote URL's scheme",
    },
    "crates/repo-service/src/credentials.rs": {
        'format!("https://{host}{port}/{username}")': "credential key for the user's remote host",
        'url.starts_with("https://") || url.starts_with("http://")': "checks a remote URL's scheme",
    },
    "frontend/src/lib/messageLinks.ts": {
        "return `https://${forge.host}/": "issue link on the remote's host, opened on click (#612)",
        "`http://`/`https://` URLs": "comment naming the schemes",
    },
}

# Local addresses and documentation names: never a contact with a third party.
IGNORED_HOSTS = {"localhost", "127.0.0.1", "[::1]", "example.com", "example.net", "example.org"}
IGNORED_SUFFIXES = (".test", ".example", ".invalid", ".localhost", ".example.com", ".example.net",
                    ".example.org")

HOST = r"(\[[0-9a-f:]+\]|[a-z0-9-]+(?:\.[a-z0-9-]+)*)"
URL_RE = re.compile(r"\b(?:https?|wss?)://" + HOST, re.IGNORECASE)
# `//cdn.host/x.js`: a `//` not after a word character, `:` or `/`, then a dotted name.
PROTOCOL_RELATIVE_RE = re.compile(r"(?<![\w:/])//([a-z0-9-]+(?:\.[a-z0-9-]+)+)", re.IGNORECASE)
# A scheme followed by something that starts no host: interpolation, a closing quote, `+`.
DYNAMIC_URL_RE = re.compile(r"\b(?:https?|wss?)://(?=[{$\"'`+(])", re.IGNORECASE)
SOURCE_SUFFIXES = {".rs", ".ts", ".tsx", ".js", ".mjs", ".css", ".html", ".json", ".svg"}
TEST_DIRS = {"tests", "__tests__", "fixtures", "test-support"}
TEST_MOD_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;\s*$")


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


def rust_test_module_files(path: Path, lines: list[str]) -> set[Path]:
    """The files of the `#[cfg(test)] mod name;` modules `path` declares."""
    folder = path.parent if path.name in ("lib.rs", "main.rs", "mod.rs") else path.with_suffix("")
    files: set[Path] = set()
    for attr, decl in zip(lines, lines[1:]):
        match = TEST_MOD_RE.match(decl)
        if attr.strip() == "#[cfg(test)]" and match:
            files |= {folder / f"{match.group(1)}.rs", folder / match.group(1)}
    return files


def is_ignored(host: str) -> bool:
    return host in IGNORED_HOSTS or host.endswith(IGNORED_SUFFIXES) or host in NON_NETWORK_HOSTS


def source_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for src in sorted((root / "crates").glob("*/src")):
        files.extend(src.rglob("*.rs"))
    for folder in ("frontend/src", "extension/src"):
        base = root / folder
        if base.is_dir():
            files.extend(p for p in base.rglob("*") if p.suffix in SOURCE_SUFFIXES)
    public = root / "frontend/public"
    if public.is_dir():
        files.extend(public.rglob("*"))
    files.extend((root / "frontend").glob("vite*.ts"))
    files.extend((root / "crates/tauri-app/capabilities").glob("*.json"))
    for name in ("frontend/index.html", "extension/package.json", "crates/tauri-app/tauri.conf.json"):
        files.append(root / name)
    files = [p for p in files if p.is_file() and not is_test_file(p.relative_to(root))]
    test_modules: set[Path] = set()
    for path in files:
        if path.suffix == ".rs":
            test_modules |= rust_test_module_files(path, read(path).splitlines())
    return sorted(
        {p for p in files if not any(p == m or m in p.parents for m in test_modules)}
    )


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def scan(root: Path) -> tuple[dict[str, list[str]], list[str]]:
    """Host -> the files that name it, and the errors for unlisted dynamic URLs."""
    found: dict[str, list[str]] = {}
    errors: list[str] = []
    used_snippets: set[tuple[str, str]] = set()
    for path in source_files(root):
        rel = path.relative_to(root).as_posix()
        lines = read(path).splitlines()
        if path.suffix == ".rs":
            lines = strip_rust_test_modules(lines)
        allowed = DYNAMIC_URLS.get(rel, {})
        for number, line in enumerate(lines, 1):
            for match in [*URL_RE.finditer(line), *PROTOCOL_RELATIVE_RE.finditer(line)]:
                host = match.group(1).lower()
                if not is_ignored(host):
                    found.setdefault(host, []).append(rel)
            if DYNAMIC_URL_RE.search(line):
                snippets = [s for s in allowed if s in line]
                used_snippets.update((rel, s) for s in snippets)
                if not snippets:
                    errors.append(
                        f"{rel}: a URL without a literal host: {line.strip()!r}. Write the host "
                        "into the URL, or add the line to DYNAMIC_URLS in "
                        "scripts/check-privacy-hosts.py with the reason"
                    )
    for rel, snippets in sorted(DYNAMIC_URLS.items()):
        for snippet in snippets:
            if (rel, snippet) not in used_snippets:
                errors.append(f"DYNAMIC_URLS entry {snippet!r} for {rel} matches no line: remove it")
    return found, errors


def find_hosts(root: Path) -> dict[str, list[str]]:
    return scan(root)[0]


def check(root: Path) -> list[str]:
    found, errors = scan(root)
    notice_path = root / "docs/guide/privacy.md"
    notice = notice_path.read_text(encoding="utf-8") if notice_path.is_file() else ""
    for host, places in sorted(found.items()):
        if host not in ALLOWED_HOSTS:
            errors.append(
                f"{host} (in {', '.join(sorted(set(places)))}) is not on ALLOWED_HOSTS: document "
                "the contact in docs/guide/privacy.md and add it to scripts/check-privacy-hosts.py"
                " (or, if the app never contacts it, to NON_NETWORK_HOSTS)"
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
