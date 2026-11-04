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
4. The TextMate grammars the syntax highlighting bundles: highlight.ts's Shiki languages plus
   the grammars they import, minus frontend/src/lib/excludedGrammars.ts, with their upstream
   licence texts from tm-grammars' NOTICE (frontend devDependency, pinned to the release
   @shikijs/langs was built from; checked). A grammar under a licence outside the policy fails
   unless GRAMMAR_EXCEPTIONS names it.

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


def _run(cmd: list[str], run=subprocess.run) -> str:
    return _run_in(cmd, ROOT, run)


def _run_in(cmd: list[str], cwd: Path, run=subprocess.run) -> str:
    try:
        # cargo-about's JSON carries non-ASCII licence texts; never decode with the locale's
        # default (cp1252 on Windows).
        return run(cmd, cwd=cwd, check=True, capture_output=True, text=True, encoding="utf-8").stdout
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
    except (OSError, subprocess.CalledProcessError, checker.CheckError) as e:
        raise NoticesError(f"`pnpm licenses list --prod` failed in {project}/: {e}") from e


# --- Syntax highlighting grammars (Shiki, tm-grammars) ---------------------------------------

FRONTEND = ROOT / "frontend"
HIGHLIGHT_TS = FRONTEND / "src" / "lib" / "highlight.ts"
EXCLUDED_GRAMMARS_TS = FRONTEND / "src" / "lib" / "excludedGrammars.ts"
GRAMMAR_NOTICE_DIR = ROOT / "scripts" / "notices" / "grammars"


@dataclass(frozen=True)
class GrammarException:
    license: str  # label shown in the notices
    text_file: str  # in scripts/notices/grammars/
    reason: str


# Shipped grammars whose upstream licence is not on the allowed list. Each is an owner decision,
# recorded in docs/LICENSE_COMPLIANCE.md ("Syntax highlighting grammars"); exact grammar names
# only. An entry that matches no shipped grammar fails the generator.
TEXTMATE_BUNDLE = GrammarException(
    "TextMate bundle licence",
    "textmate-bundle-license.txt",
    "TextMate bundle permission notice; owner decision 2026-11-04 (#301)",
)
GRAMMAR_EXCEPTIONS: dict[str, GrammarException] = {
    "yaml": TEXTMATE_BUNDLE,  # textmate/yaml.tmbundle
    "toml": TEXTMATE_BUNDLE,  # textmate/toml.tmbundle
}

# Keys Shiki adds to a tm-grammars grammar when it builds @shikijs/langs.
SHIKI_ADDED_KEYS = ("aliases", "embeddedLangs", "embeddedLangsLazy")


def excluded_grammars(source: str) -> set[str]:
    """The `name`s in excludedGrammars.ts's EXCLUDED_GRAMMARS array."""
    block = re.search(r"EXCLUDED_GRAMMARS = \[(.*?)\] as const", source, re.DOTALL)
    if block is None:
        raise NoticesError(f"{EXCLUDED_GRAMMARS_TS.relative_to(ROOT)}: EXCLUDED_GRAMMARS not found")
    return set(re.findall(r'name: "([\w.+-]+)"', block.group(1)))


def grammar_imports(module_source: str) -> list[str]:
    """The grammars a @shikijs/langs module pulls in: any `from './name.mjs'` (import or
    re-export)."""
    return re.findall(r"""\bfrom\s*['"]\./([\w.+-]+)\.mjs['"]""", module_source)


def loader_roots(highlight_source: str) -> list[str]:
    """The grammar modules highlight.ts's LOADERS import. Fails unless every LOADERS entry was
    read, so a reformatted loader can't silently drop a grammar from the notices."""
    block = re.search(r"const LOADERS\b[^\n]*?=\s*\{(.*?)\n\};", highlight_source, re.DOTALL)
    if block is None:
        raise NoticesError(f"no LOADERS object found in {HIGHLIGHT_TS.relative_to(ROOT)}")
    entries = re.findall(r"^\s*[\w$\"'-]+\s*:", block.group(1), re.MULTILINE)
    roots = re.findall(r'import\("shiki/langs/([\w.+-]+)\.mjs"\)', block.group(1))
    if len(roots) != len(entries):
        raise NoticesError(
            f"{HIGHLIGHT_TS.relative_to(ROOT)} has {len(entries)} LOADERS entries but {len(roots)} "
            'read as `import("shiki/langs/<name>.mjs")`; update loader_roots'
        )
    return roots


def shipped_grammars(highlight_source: str, read_module, excluded: set[str]) -> set[str]:
    """Every grammar the app bundles: highlight.ts's loaders plus what they import, without the
    excluded ones (vite.grammars.ts cuts those imports)."""
    roots = loader_roots(highlight_source)
    if not roots:
        raise NoticesError(f"no shiki/langs imports found in {HIGHLIGHT_TS.relative_to(ROOT)}")
    shipped: set[str] = set()
    todo = [name for name in roots if name not in excluded]
    while todo:
        name = todo.pop()
        if name in shipped:
            continue
        shipped.add(name)
        todo += [dep for dep in grammar_imports(read_module(name)) if dep not in excluded]
    return shipped


def shiki_grammar(module_source: str) -> dict:
    """The grammar JSON inside a @shikijs/langs module (`JSON.parse("...")`)."""
    match = re.search(r'JSON\.parse\(("(?:[^"\\]|\\.)*")\)', module_source)
    if match is None:
        raise NoticesError("no JSON.parse(...) grammar found in a @shikijs/langs module")
    return json.loads(json.loads(match.group(1)))


def grammar_drift(shiki: dict, upstream: dict) -> list[str]:
    """Top-level keys where Shiki's copy of a grammar differs from tm-grammars' copy."""
    keys = (set(shiki) | set(upstream)) - set(SHIKI_ADDED_KEYS)
    return sorted(k for k in keys if shiki.get(k) != upstream.get(k))


def parse_grammar_notice(notice: str) -> dict[str, tuple[str, str]]:
    """tm-grammars' NOTICE -> {grammar name: (SPDX id, licence text)}."""
    sections: dict[str, tuple[str, str]] = {}
    # A section starts with a line of "=" followed by its "Files:" line. Licence texts also
    # contain "=" lines (title underlines), which are not followed by "Files:".
    for block in re.split(r"^=+\n(?=Files:)", notice, flags=re.MULTILINE)[1:]:
        head, sep, text = block.partition("\n" + "-" * 20)
        if not sep:
            continue
        files = re.search(r"^Files:\s+(.+)$", head, re.MULTILINE)
        spdx = re.search(r"^SPDX:\s+(.+)$", head, re.MULTILINE)
        if files is None:
            continue
        text = text.split("\n", 1)[1] if "\n" in text else ""
        for file in files.group(1).split(","):
            name = file.strip().removesuffix(".json")
            sections[name] = (spdx.group(1).strip() if spdx else "", text)
    return sections


def grammar_entries(
    shipped: set[str],
    metadata: dict[str, dict],
    notice: dict[str, tuple[str, str]],
    allowed: frozenset[str],
    read_exception_text=lambda name: (GRAMMAR_NOTICE_DIR / name).read_text(encoding="utf-8"),
    exceptions: dict[str, GrammarException] | None = None,
) -> tuple[list[tuple[str, str, str]], list[str]]:
    """Shipped grammars -> ([(grammar, licence, text)], problems)."""
    checker = _load_license_checker()
    exceptions = GRAMMAR_EXCEPTIONS if exceptions is None else exceptions
    entries, problems = [], []
    for name in sorted(shipped):
        meta = metadata.get(name)
        if name in exceptions:
            exc = exceptions[name]
            entries.append((name, exc.license, read_exception_text(exc.text_file)))
            continue
        if meta is None:
            problems.append(f"grammar {name} has no tm-grammars metadata entry")
            continue
        license_id = meta.get("license") or ""
        if not license_id or not checker.satisfies(license_id, allowed):
            problems.append(
                f"grammar {name} ({meta.get('source', '?')}) is licensed {license_id or 'nowhere'}, which is "
                "not an allowed licence and has no GRAMMAR_EXCEPTIONS entry; it needs an owner decision, "
                "or add it to frontend/src/lib/excludedGrammars.ts"
            )
            continue
        if name not in notice:
            problems.append(f"grammar {name} has no section in tm-grammars' NOTICE")
            continue
        if not notice[name][1].strip():
            problems.append(f"grammar {name} has an empty licence text in tm-grammars' NOTICE")
            continue
        entries.append((name, license_id, notice[name][1]))
    for name in sorted(set(exceptions) - shipped):
        problems.append(f"GRAMMAR_EXCEPTIONS entry {name} matches no shipped grammar; remove it")
    return entries, problems


def grammar_sources(shipped: set[str], metadata: dict[str, dict]) -> list[str]:
    lines = []
    for name in sorted(shipped):
        meta = metadata.get(name, {})
        lines.append(f"- {meta.get('displayName', name)} (`{name}`): {meta.get('source', 'source unknown')}")
    return lines


def _node_json(script: str) -> object:
    return json.loads(_run_in(["node", "--input-type=module", "-e", script], FRONTEND))


def collect_grammars() -> tuple[list[tuple[str, str, str]], list[str]]:
    """The shipped grammars' notice entries and their source list. Raises NoticesError."""
    tm_dir = FRONTEND / "node_modules" / "tm-grammars"
    shiki_dir = (FRONTEND / "node_modules" / "shiki").resolve()
    langs_dir = shiki_dir.parent / "@shikijs" / "langs" / "dist"
    if not tm_dir.is_dir() or not langs_dir.is_dir():
        raise NoticesError("frontend/node_modules lacks tm-grammars or @shikijs/langs; run `pnpm install` in frontend/")

    def read_module(name: str) -> str:
        path = langs_dir / f"{name}.mjs"
        if not path.is_file():
            raise NoticesError(f"@shikijs/langs has no {name}.mjs")
        return path.read_text(encoding="utf-8")

    excluded = excluded_grammars(EXCLUDED_GRAMMARS_TS.read_text(encoding="utf-8"))
    shipped = shipped_grammars(HIGHLIGHT_TS.read_text(encoding="utf-8"), read_module, excluded)

    # The tm-grammars pin must be the release @shikijs/langs was built from (Shiki only asks
    # for a version range): every shipped grammar must be identical in both.
    tm_version = json.loads((tm_dir / "package.json").read_text(encoding="utf-8"))["version"]
    langs_version = json.loads((langs_dir.parent / "package.json").read_text(encoding="utf-8"))["version"]
    problems = []
    for name in sorted(shipped):
        upstream = tm_dir / "grammars" / f"{name}.json"
        if not upstream.is_file():
            problems.append(f"tm-grammars {tm_version} has no {name} grammar")
            continue
        drift = grammar_drift(shiki_grammar(read_module(name)), json.loads(upstream.read_text(encoding="utf-8")))
        if drift:
            problems.append(f"grammar {name} differs between @shikijs/langs {langs_version} and tm-grammars {tm_version} ({', '.join(drift)})")
    if problems:
        raise NoticesError(
            "\n".join(problems)
            + f"\nPin frontend's tm-grammars devDependency to the version @shikijs/langs {langs_version} was built from."
        )

    metadata = {g["name"]: g for g in _node_json('import { grammars } from "tm-grammars"; console.log(JSON.stringify(grammars))')}
    notice = parse_grammar_notice((tm_dir / "NOTICE").read_text(encoding="utf-8"))
    entries, problems = grammar_entries(shipped, metadata, notice, _load_license_checker().ALLOWED)
    if problems:
        raise NoticesError("\n".join(problems))
    return entries, grammar_sources(shipped, metadata)


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
GRAMMAR_INTRO = (
    "TextMate grammars the app's syntax highlighting bundles (through Shiki's `@shikijs/langs`, "
    "built from `tm-grammars`), each with its upstream source and licence:"
)
FOOTER = f"<!-- Generated by scripts/generate-third-party-notices.py. Do not edit; run `{REGENERATE}`. -->"


def render(
    header: str,
    rust: list[LicenseGroup],
    npm: list[LicenseGroup],
    grammars: list[LicenseGroup] | None = None,
    grammar_source_lines: list[str] | None = None,
) -> str:
    parts = [
        normalize_text(header),
        render_section("Rust crates", RUST_INTRO, rust).rstrip(),
        render_section("JavaScript packages", NPM_INTRO, npm).rstrip(),
    ]
    if grammars:
        intro = GRAMMAR_INTRO + "\n\n" + "\n".join(grammar_source_lines or [])
        parts.append(render_section("Syntax highlighting grammars", intro, grammars).rstrip())
    parts.append(FOOTER)
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
    grammars, grammar_source_lines = collect_grammars()
    return render(header, group_by_text(rust), group_by_text(npm), group_by_text(grammars), grammar_source_lines)


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
