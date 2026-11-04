#!/usr/bin/env python3
"""Generate THIRD_PARTY_NOTICES.md, or check that it is current (#301, #202).

The file ships with the desktop app (a Tauri resource, and the "Third-party licences" dialog)
and in every VSCode extension package. It has three parts:

1. `scripts/notices/native-libraries.md`, kept by hand: the native C libraries the Rust build
   vendors through `-sys` crates (libgit2, libssh2, OpenSSL, zlib), which cargo-about can't
   see, and the MPL-2.0 source notice for option-ext. Each `-sys` crate version it names must
   match Cargo.lock.
2. The Rust crates linked into the desktop app and the VSCode sidecar, with their licence
   texts, from `cargo about generate --format json` (configuration: `about.toml`).
3. The npm packages bundled into the shipped JavaScript: the production trees of `frontend/`
   and `extension/` (`pnpm licenses list --prod`), without `@types/*` type-only packages,
   with the licence files from each package.

Usage, from the repository root, after `pnpm install` in frontend/ and extension/:

    python3 scripts/generate-third-party-notices.py          # rewrite THIRD_PARTY_NOTICES.md
    python3 scripts/generate-third-party-notices.py --check  # CI: fail if it is out of date

Needs cargo-about (`cargo install cargo-about --locked --features cli --version <the
CARGO_ABOUT_VERSION in .github/workflows/ci.yml>`).
"""

from __future__ import annotations

import argparse
import difflib
import fnmatch
import importlib.util
import json
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "THIRD_PARTY_NOTICES.md"
HEADER = ROOT / "scripts" / "notices" / "native-libraries.md"
REGENERATE = "python3 scripts/generate-third-party-notices.py"

# The `-sys` crates whose vendored C library the hand-maintained header describes.
NATIVE_SYS_CRATES = ("libgit2-sys", "libssh2-sys", "openssl-src", "libz-sys")

# The binaries we ship. Their dependency trees cover every other workspace crate.
SHIPPED_PACKAGES = ("tauri-app", "vscode-sidecar")

# The pnpm projects whose production tree is bundled into what we ship.
NPM_PROJECTS = ("frontend", "extension")

# npm packages that ship no licence file of their own, mapped to the upstream licence file kept
# in scripts/notices/npm/ (copied unchanged from the package's source repository). An entry
# that matches no package fails the generator, so the table can't go stale.
NPM_FALLBACK_TEXTS = {
    # The plugins' npm packages carry only LICENSE.spdx; the MIT text is in their repository,
    # https://github.com/tauri-apps/plugins-workspace (LICENSE_MIT).
    "@tauri-apps/plugin-*": "tauri-plugins-workspace-LICENSE_MIT.txt",
}
NPM_FALLBACK_DIR = ROOT / "scripts" / "notices" / "npm"

# LICENSE, LICENCE.md, COPYING.txt, LICENSE-MIT, NOTICE, ... but not license-checker.js.
LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|notice)(-[a-z0-9-]+)?(\.(md|txt|markdown|rst))?$", re.IGNORECASE)


def _load_license_checker():
    path = ROOT / "scripts" / "check-dependency-licenses.py"
    spec = importlib.util.spec_from_file_location("check_dependency_licenses", path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class NoticesError(Exception):
    """The notices can't be generated (a tool failed, or the header no longer matches)."""


# --- Licence groups ---------------------------------------------------------------------


@dataclass
class LicenseGroup:
    """One licence text and the packages it covers."""

    license: str  # SPDX id or expression
    text: str
    users: set[str] = field(default_factory=set)  # "name version"


def normalize_text(text: str) -> str:
    """Unify line endings and trailing whitespace, so the output is the same on every OS."""
    lines = [line.rstrip() for line in text.replace("\r\n", "\n").replace("\r", "\n").split("\n")]
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    return "\n".join(lines)


def group_by_text(entries: list[tuple[str, str, str]]) -> list[LicenseGroup]:
    """[(user, licence, text)] -> groups sharing one exact text, in a stable order."""
    groups: dict[tuple[str, str], LicenseGroup] = {}
    for user, license_id, text in entries:
        text = normalize_text(text)
        key = (license_id, text)
        groups.setdefault(key, LicenseGroup(license_id, text)).users.add(user)
    return sorted(groups.values(), key=lambda g: (g.license, sorted(g.users)[0], g.text))


def fence(text: str) -> str:
    """A code fence longer than any backtick run inside `text`."""
    longest = max((len(m) for m in re.findall(r"`+", text)), default=0)
    return "`" * max(3, longest + 1)


def render_section(title: str, intro: str, groups: list[LicenseGroup]) -> str:
    out = [f"## {title}", "", intro, ""]
    by_license: dict[str, set[str]] = {}
    for group in groups:
        by_license.setdefault(group.license, set()).update(group.users)
    for license_id in sorted(by_license):
        users = sorted(by_license[license_id], key=_user_sort_key)
        out.append(f"- **{license_id}**: {', '.join(users)}")
    out.append("")
    for group in groups:
        users = sorted(group.users, key=_user_sort_key)
        out += [f"### {group.license}: {users[0]}" + (f" and {len(users) - 1} more" if len(users) > 1 else ""), ""]
        out += [f"Used by: {', '.join(users)}.", ""]
        marks = fence(group.text)
        out += [marks + "text", group.text, marks, ""]
    return "\n".join(out)


def _user_sort_key(user: str) -> tuple[str, str]:
    name, _, version = user.rpartition(" ")
    return (name.lower(), version)


# --- Rust (cargo-about, cargo tree) -----------------------------------------------------


def rust_entries(about: dict, shipped: set[tuple[str, str]]) -> list[tuple[str, str, str]]:
    """`cargo about generate --format json` output -> [(crate, licence, text)].

    Keeps only the crates in `shipped` (name, version): cargo-about also lists build-time
    crates reached through proc macros, which are not linked into anything we ship. Raises
    NoticesError if a shipped crate has no licence in the output.
    """
    entries = []
    covered = set()
    for lic in about.get("licenses", []):
        for used in lic.get("used_by", []):
            key = (used["crate"]["name"], used["crate"]["version"])
            if key in shipped:
                covered.add(key)
                entries.append((f"{key[0]} {key[1]}", lic["id"], lic["text"]))
    missing = sorted(shipped - covered)
    if missing:
        raise NoticesError(f"cargo-about found no licence text for: {', '.join(f'{n} {v}' for n, v in missing)}")
    return entries


def parse_cargo_tree(output: str, workspace: set[str]) -> set[tuple[str, str]]:
    """`cargo tree --prefix none -f '{p}'` lines -> {(name, version)}, without our own crates."""
    shipped = set()
    for line in output.splitlines():
        parts = line.split()
        if len(parts) < 2 or not parts[1].startswith("v"):
            continue
        if parts[0] not in workspace:
            shipped.add((parts[0], parts[1][1:]))
    return shipped


def _run(cmd: list[str]) -> str:
    try:
        return subprocess.run(cmd, cwd=ROOT, check=True, capture_output=True, text=True).stdout
    except FileNotFoundError as e:
        raise NoticesError(f"{cmd[0]} not found") from e
    except subprocess.CalledProcessError as e:
        raise NoticesError(f"`{' '.join(cmd)}` failed:\n{e.stderr.strip()}") from e


def about_targets() -> list[str]:
    """The `targets` list in about.toml (the platforms we ship)."""
    text = (ROOT / "about.toml").read_text(encoding="utf-8")
    block = re.search(r"^targets = \[(.*?)\]", text, re.MULTILINE | re.DOTALL)
    if block is None:
        raise NoticesError("about.toml has no `targets` list")
    return re.findall(r'"([^"]+)"', block.group(1))


def shipped_crates(workspace: set[str]) -> set[tuple[str, str]]:
    """Every crate the shipped binaries link, on every platform we ship."""
    cmd = ["cargo", "tree", "--locked", "-e", "normal,no-proc-macro", "--prefix", "none", "-f", "{p}"]
    for package in SHIPPED_PACKAGES:
        cmd += ["-p", package]
    for target in about_targets():
        cmd += ["--target", target]
    return parse_cargo_tree(_run(cmd), workspace)


def run_cargo_about() -> dict:
    return json.loads(
        _run(["cargo", "about", "generate", "--format", "json", "--locked", "--fail", "--workspace",
              "--config", str(ROOT / "about.toml")])
    )


# --- npm (pnpm) --------------------------------------------------------------------------


def package_license_texts(directory: Path) -> list[tuple[str, str]]:
    """The licence/notice files at the top of an installed package: [(file name, text)]."""
    if not directory.is_dir():
        return []
    found = []
    for path in sorted(directory.iterdir(), key=lambda p: p.name.lower()):
        if path.is_file() and LICENSE_FILE.match(path.name):
            found.append((path.name, path.read_text(encoding="utf-8", errors="replace")))
    return found


def npm_entries(
    licenses_json: dict, read_texts=package_license_texts, fallbacks: dict[str, str] | None = None
) -> tuple[list[tuple[str, str, str]], set[str]]:
    """`pnpm licenses list --json --prod` output -> ([(package, licence, text)], used fallbacks).

    A package without a licence file gets the text its `fallbacks` entry names, if any.
    """
    checker = _load_license_checker()
    fallbacks = NPM_FALLBACK_TEXTS if fallbacks is None else fallbacks
    used: set[str] = set()
    entries = []
    for group, packages in licenses_json.items():
        for pkg in packages:
            if pkg["name"].startswith("@types/"):
                continue  # type declarations only; nothing of them is bundled
            license_id = checker.normalize_license(pkg.get("license", group))
            paths = pkg.get("paths") or []
            for index, version in enumerate(pkg.get("versions", [])):
                directory = Path(paths[index]) if index < len(paths) else None
                texts = read_texts(directory) if directory is not None else []
                if not texts:
                    pattern = next((p for p in fallbacks if fnmatch.fnmatchcase(pkg["name"], p)), None)
                    if pattern is not None:
                        used.add(pattern)
                        texts = [(fallbacks[pattern], (NPM_FALLBACK_DIR / fallbacks[pattern]).read_text(encoding="utf-8"))]
                if texts:
                    text = "\n\n".join(body if len(texts) == 1 else f"{name}:\n\n{body}" for name, body in texts)
                else:
                    text = f"The package ships no licence file. Its package.json declares: {license_id}."
                entries.append((f"{pkg['name']} {version}", license_id, text))
    return entries, used


def run_pnpm_licenses(project: str) -> dict:
    checker = _load_license_checker()
    if not (ROOT / project / "node_modules").is_dir():
        raise NoticesError(f"{project}/node_modules is missing; run `pnpm install --frozen-lockfile` in {project}/")
    try:
        return checker.pnpm_licenses(project, True)
    except (OSError, subprocess.CalledProcessError) as e:
        raise NoticesError(f"`pnpm licenses list --prod` failed in {project}/: {e}") from e


# --- Native libraries (hand-maintained header) -------------------------------------------


def lock_versions(lock_text: str, names: tuple[str, ...]) -> dict[str, set[str]]:
    """Cargo.lock -> {crate: versions}, for the named crates."""
    versions: dict[str, set[str]] = {name: set() for name in names}
    for block in lock_text.split("[[package]]"):
        name = re.search(r'^name = "([^"]+)"', block, re.MULTILINE)
        version = re.search(r'^version = "([^"]+)"', block, re.MULTILINE)
        if name and version and name.group(1) in versions:
            versions[name.group(1)].add(version.group(1))
    return versions


def header_versions(header: str, names: tuple[str, ...]) -> dict[str, set[str]]:
    """Versions the header names as "the `<crate>` crate <version>"."""
    versions: dict[str, set[str]] = {name: set() for name in names}
    for name, version in re.findall(r"the `([A-Za-z0-9_-]+)`\s+crate\s+([0-9][0-9A-Za-z.+-]*[0-9A-Za-z])", header):
        if name in versions:
            versions[name].add(version)
    return versions


def native_version_problems(header: str, lock_text: str) -> list[str]:
    problems = []
    in_lock = lock_versions(lock_text, NATIVE_SYS_CRATES)
    in_header = header_versions(header, NATIVE_SYS_CRATES)
    for name in NATIVE_SYS_CRATES:
        if in_lock[name] != in_header[name]:
            problems.append(
                f"{HEADER.relative_to(ROOT)} names {name} {sorted(in_header[name]) or 'nowhere'}, "
                f"Cargo.lock has {sorted(in_lock[name]) or 'none'}. Check the vendored library's "
                "licence text and version, update that file, then regenerate."
            )
    return problems


# --- Assembly -----------------------------------------------------------------------------

RUST_INTRO = (
    "Rust crates linked into the desktop app and the VSCode extension's `bin/vscode-sidecar`, "
    "on every platform Browsitory is built for. Where a crate offers a choice of licences, "
    "Browsitory uses it under the one shown."
)
NPM_INTRO = "JavaScript packages bundled into the app's user interface."
FOOTER = f"<!-- Generated by scripts/generate-third-party-notices.py. Do not edit; run `{REGENERATE}`. -->"


def render(header: str, rust: list[LicenseGroup], npm: list[LicenseGroup]) -> str:
    parts = [
        normalize_text(header),
        render_section("Rust crates", RUST_INTRO, rust).rstrip(),
        render_section("JavaScript packages", NPM_INTRO, npm).rstrip(),
        FOOTER,
    ]
    return "\n\n".join(parts) + "\n"


def workspace_members() -> set[str]:
    out = _run(["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"])
    return {pkg["name"] for pkg in json.loads(out)["packages"]}


def generate() -> str:
    header = HEADER.read_text(encoding="utf-8")
    problems = native_version_problems(header, (ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    if problems:
        raise NoticesError("\n".join(problems))
    workspace = workspace_members()
    rust = rust_entries(run_cargo_about(), shipped_crates(workspace))
    npm = []
    used_fallbacks: set[str] = set()
    for project in NPM_PROJECTS:
        entries, used = npm_entries(run_pnpm_licenses(project))
        npm += entries
        used_fallbacks |= used
    unused = sorted(set(NPM_FALLBACK_TEXTS) - used_fallbacks)
    if unused:
        raise NoticesError(f"NPM_FALLBACK_TEXTS entries match no package without a licence file: {', '.join(unused)}")
    return render(header, group_by_text(rust), group_by_text(npm))


def main(argv: list[str] | None = None, generate_fn=generate) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="fail if THIRD_PARTY_NOTICES.md is out of date")
    args = parser.parse_args(argv)
    try:
        content = generate_fn()
    except NoticesError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    current = OUTPUT.read_text(encoding="utf-8").replace("\r\n", "\n") if OUTPUT.exists() else ""
    if args.check:
        if current == content:
            print(f"{OUTPUT.name} is up to date ({len(content.encode())} bytes).")
            return 0
        diff = list(difflib.unified_diff(current.splitlines(), content.splitlines(), "checked in", "generated", lineterm="", n=1))
        print("\n".join(diff[:60]), file=sys.stderr)
        if len(diff) > 60:
            print(f"... ({len(diff) - 60} more diff lines)", file=sys.stderr)
        print(
            f"error: {OUTPUT.name} no longer matches the dependency trees. "
            f"Run `{REGENERATE}` (after `pnpm install` in frontend/ and extension/) and commit the result. "
            "See docs/LICENSE_COMPLIANCE.md.",
            file=sys.stderr,
        )
        return 1
    OUTPUT.write_text(content, encoding="utf-8", newline="\n")
    print(f"Wrote {OUTPUT.name} ({len(content.encode())} bytes).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
