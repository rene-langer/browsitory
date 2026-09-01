#!/usr/bin/env python3
"""Fail if a direct dependency isn't documented in docs/LICENSE_COMPLIANCE.md.

Compares the direct dependencies declared in crates/*/Cargo.toml and
frontend/package.json / extension/package.json / e2e/package.json against the first column of that
doc's Rust/JavaScript tables. Doesn't check license values themselves (that
step is still manual, per the doc's "Process" section) — only that nothing
shipped has silently gone undocumented, per SEC-004/MAINT-003
(github.com/rene-langer/browsitory/issues/25).
"""

import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOC = ROOT / "docs" / "LICENSE_COMPLIANCE.md"

WORKSPACE_CRATES = {"git-core", "config", "repo-service", "tauri-app", "vscode-sidecar"}


def rust_deps() -> set[str]:
    names: set[str] = set()
    for cargo_toml in ROOT.glob("crates/*/Cargo.toml"):
        data = tomllib.loads(cargo_toml.read_text())
        for section in ("dependencies", "dev-dependencies", "build-dependencies"):
            for name in data.get(section, {}):
                if name not in WORKSPACE_CRATES:
                    names.add(name)
    return names


def js_deps(package_json: Path) -> set[str]:
    data = json.loads(package_json.read_text())
    names: set[str] = set()
    for section in ("dependencies", "devDependencies"):
        names.update(data.get(section, {}))
    return names


def doc_table_names(text: str, section_header: str, stop_headers: tuple[str, ...]) -> set[str]:
    start = text.index(section_header) + len(section_header)
    end = len(text)
    for stop in stop_headers:
        idx = text.find(stop, start)
        if idx != -1:
            end = min(end, idx)
    body = text[start:end]
    names: set[str] = set()
    for line in body.splitlines():
        if not line.startswith("| "):
            continue
        cell = line.split("|")[1].strip()
        if cell in ("Crate", "Package") or set(cell) == {"-"}:
            continue
        # Table rows like "keyring 4.1.6" or "tauri-plugin-updater 2" carry a trailing
        # version after the name; the JS tables never do, so splitting on whitespace is safe
        # for both.
        names.add(cell.split(" ")[0])
    return names


def main() -> int:
    problems: list[str] = []
    doc_text = DOC.read_text()

    documented_rust = doc_table_names(doc_text, "## Rust (`cargo info <crate>`)", ("\n## ",))
    missing_rust = rust_deps() - documented_rust
    if missing_rust:
        problems.append(f"Rust dependencies missing from docs/LICENSE_COMPLIANCE.md: {sorted(missing_rust)}")

    documented_frontend = doc_table_names(
        doc_text,
        "## JavaScript (`npm info <package> license`)",
        ("\n## JavaScript, `extension/`", "\n## JavaScript, `e2e/`"),
    )
    missing_frontend = js_deps(ROOT / "frontend" / "package.json") - documented_frontend
    if missing_frontend:
        problems.append(f"frontend/package.json dependencies missing from docs/LICENSE_COMPLIANCE.md: {sorted(missing_frontend)}")

    documented_extension = doc_table_names(
        doc_text, "## JavaScript, `extension/` (`npm info <package> license`)", ("\n## JavaScript, `e2e/`",)
    )
    missing_extension = js_deps(ROOT / "extension" / "package.json") - documented_extension
    if missing_extension:
        problems.append(f"extension/package.json dependencies missing from docs/LICENSE_COMPLIANCE.md: {sorted(missing_extension)}")

    documented_e2e = doc_table_names(doc_text, "## JavaScript, `e2e/` (`npm info <package> license`)", ("\n## ",))
    missing_e2e = js_deps(ROOT / "e2e" / "package.json") - documented_e2e
    if missing_e2e:
        problems.append(f"e2e/package.json dependencies missing from docs/LICENSE_COMPLIANCE.md: {sorted(missing_e2e)}")

    if problems:
        for problem in problems:
            print(f"error: {problem}", file=sys.stderr)
        print(
            "Add a row to the relevant table in docs/LICENSE_COMPLIANCE.md "
            "(see its \"Process\" section) and rerun.",
            file=sys.stderr,
        )
        return 1

    print("docs/LICENSE_COMPLIANCE.md covers every direct dependency.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
