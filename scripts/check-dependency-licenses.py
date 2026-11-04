#!/usr/bin/env python3
"""Check every npm package's licence against the policy (#273).

For each pnpm project (frontend/, extension/, e2e/, extension/e2e/), the whole installed tree,
production and dev, comes from `pnpm licenses list --json`. Run `pnpm install
--frozen-lockfile` in each project first.

Rules:

- The production trees of `frontend/` and `extension/` (what ships in the app and the VSIX)
  must use only ALLOWED licences. No exception applies to them.
- Every other package (dev tooling, test harnesses) must use an ALLOWED licence or match an
  entry in EXCEPTIONS: name, version range, licence and reason, each an owner decision
  mirrored in docs/LICENSE_COMPLIANCE.md.
- `A OR B` passes when one side passes; `A AND B` needs both sides.
- An EXCEPTIONS or CLARIFICATIONS entry that matches nothing fails the check, so the table
  can't go stale.

pnpm installs only the current platform's native binaries (esbuild, rolldown, lightningcss,
@tauri-apps/cli, @vscode/vsce-sign, ...). Packages in the lockfile but not installed are looked
up in the npm registry by exact name@version (published versions can't change), so the check
covers every platform no matter which machine runs it.
"""

from __future__ import annotations

import argparse
import fnmatch
import json
import re
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Keep in sync with AGENTS.md ("License policy"), docs/LICENSE_COMPLIANCE.md and deny.toml.
ALLOWED = frozenset(
    {
        "MIT",
        "Apache-2.0",
        "ISC",
        "BSD-2-Clause",
        "BSD-3-Clause",
        "MIT-0",
        "Unicode-3.0",
        "Zlib",
        "BlueOak-1.0.0",
        "PSF-2.0",
        "Python-2.0",
    }
)

# (project directory, its production tree ships)
PROJECTS = (
    ("frontend", True),
    ("extension", True),
    ("e2e", False),
    ("extension/e2e", False),
)
# Not a member of extension/'s workspace; AGENTS.md installs it with --ignore-workspace too.
IGNORE_WORKSPACE = {"extension/e2e"}

UNKNOWN = "Unknown"


@dataclass(frozen=True)
class LicenseException:
    name: str  # exact package name, or a glob such as "lightningcss-*"
    versions: str  # "*", or a version prefix such as "1" (1.x.y) or "1.33" (1.33.x)
    license: str  # the one licence term this entry accepts
    reason: str


@dataclass(frozen=True)
class Clarification:
    name: str
    versions: str
    license: str  # replaces the declared licence
    reason: str


# Dev-only packages under a licence outside ALLOWED. Mirrored in docs/LICENSE_COMPLIANCE.md
# ("JavaScript dev-tree exceptions"). Never add one for a production dependency.
# Globs: `*-*` matches the per-platform binary names (`lightningcss-linux-x64-gnu`,
# `@vscode/vsce-sign-darwin-arm64`), not a lookalike such as `lightningcss-plugin`.
VSCE_SIGN_REASON = (
    "Microsoft licence terms (LICENSE.txt), required by Microsoft's own publishing tool @vscode/vsce; "
    "used only to package and publish in CI, never redistributed in the VSIX or the app"
)
EXCEPTIONS: tuple[LicenseException, ...] = (
    LicenseException("lightningcss", "1", "MPL-2.0", "Vite's CSS minifier; build tool, its output isn't covered"),
    LicenseException("lightningcss-*-*", "1", "MPL-2.0", "lightningcss's per-platform native binaries"),
    LicenseException("caniuse-lite", "1", "CC-BY-4.0", "browser support data for browserslist (eslint-plugin-react-hooks > @babel/core)"),
    LicenseException("mdn-data", "2", "CC0-1.0", "CSS data for css-tree (jsdom, tests only)"),
    LicenseException("spdx-license-ids", "3", "CC0-1.0", "SPDX id list for package metadata tools (@vscode/vsce, @wdio/cli)"),
    LicenseException("spdx-exceptions", "2", "CC-BY-3.0", "SPDX exception list for package metadata tools (@vscode/vsce, @wdio/cli)"),
    LicenseException("tslib", "2", "0BSD", "TypeScript runtime helpers inside @vscode/vsce's and WebdriverIO's dependencies"),
    LicenseException("stream-buffers", "3", "Unlicense", "@wdio/local-runner (e2e only)"),
    LicenseException("@promptbook/utils", "0", "CC-BY-4.0", "@wdio/utils > locate-app (e2e only)"),
    LicenseException("@azu/style-format", "1", "WTFPL", "@vscode/vsce > secretlint formatter (packaging only)"),
    LicenseException("binaryextensions", "6", "Artistic-2.0", "@vscode/vsce > secretlint > istextorbinary (packaging only)"),
    LicenseException("editions", "6", "Artistic-2.0", "@vscode/vsce > secretlint > istextorbinary (packaging only)"),
    LicenseException("istextorbinary", "9", "Artistic-2.0", "@vscode/vsce > secretlint (packaging only)"),
    LicenseException("textextensions", "6", "Artistic-2.0", "@vscode/vsce > secretlint > istextorbinary (packaging only)"),
    LicenseException("version-range", "4", "Artistic-2.0", "@vscode/vsce > secretlint > istextorbinary > editions (packaging only)"),
    LicenseException("@vscode/vsce-sign", "2", UNKNOWN, VSCE_SIGN_REASON),
    LicenseException("@vscode/vsce-sign-*-*", "2", UNKNOWN, "@vscode/vsce-sign's per-platform binaries; " + VSCE_SIGN_REASON),
)

# Packages whose declared licence is missing or wrong.
CLARIFICATIONS: tuple[Clarification, ...] = (
    Clarification("css-value", "0.0.1", "MIT", "no licence field in package.json; its Readme.md carries the MIT licence"),
)


# --- SPDX expressions -----------------------------------------------------------------------


def normalize_license(value: object) -> str:
    """Turn a package.json/registry licence value into an SPDX expression string."""
    if isinstance(value, dict):
        value = value.get("type")
    if isinstance(value, list):
        # A legacy `licenses` array doesn't say whether the user may choose; assume every
        # licence applies (fail closed).
        parts = [normalize_license(v) for v in value]
        return " AND ".join(f"({p})" if " " in p else p for p in parts) if parts else UNKNOWN
    if not isinstance(value, str) or not value.strip():
        return UNKNOWN
    value = value.strip()
    if value.upper().startswith("SEE LICENSE IN") or value == UNKNOWN:
        return UNKNOWN
    return value


_TOKEN = re.compile(r"\s*(\(|\)|[^\s()]+)")


def _tokenize(expr: str) -> list[str]:
    pos, tokens = 0, []
    expr = expr.strip()
    while pos < len(expr):
        m = _TOKEN.match(expr, pos)
        if not m:
            break
        tokens.append(m.group(1))
        pos = m.end()
    return tokens


def satisfies(expr: str, accepted: frozenset[str] | set[str]) -> bool:
    """True if the SPDX expression can be met with licences in `accepted`.

    A `X WITH Y` term must be accepted as a whole. Unparseable input is never satisfied.
    """
    tokens = _tokenize(expr)
    pos = 0

    def peek() -> str | None:
        return tokens[pos] if pos < len(tokens) else None

    def take() -> str:
        nonlocal pos
        tok = tokens[pos]
        pos += 1
        return tok

    def term() -> bool:
        tok = peek()
        if tok is None or tok in (")", "OR", "AND", "WITH"):
            raise ValueError(expr)
        take()
        if tok == "(":
            value = or_expr()
            if peek() != ")":
                raise ValueError(expr)
            take()
            return value
        if peek() == "WITH":
            take()
            exc = peek()
            if exc is None or exc in ("(", ")", "OR", "AND", "WITH"):
                raise ValueError(expr)
            take()
            return f"{tok} WITH {exc}" in accepted
        return tok in accepted

    def and_expr() -> bool:
        value = term()
        while peek() == "AND":
            take()
            value = term() and value
        return value

    def or_expr() -> bool:
        value = and_expr()
        while peek() == "OR":
            take()
            value = and_expr() or value
        return value

    try:
        result = or_expr()
    except (ValueError, IndexError):
        return False
    return result if pos == len(tokens) else False


# --- Matching entries -----------------------------------------------------------------------


def version_matches(spec: str, version: str) -> bool:
    if spec == "*":
        return True
    want = spec.split(".")
    have = version.split("+")[0].split("-")[0].split(".")
    return have[: len(want)] == want


def _matches(entry_name: str, entry_versions: str, name: str, version: str) -> bool:
    return fnmatch.fnmatchcase(name, entry_name) and version_matches(entry_versions, version)


@dataclass(frozen=True)
class Package:
    project: str
    name: str
    version: str
    license: str
    production: bool


def check_packages(packages: list[Package]) -> tuple[list[str], list[str]]:
    """Return (problems, unused table entries)."""
    problems: list[str] = []
    used_exceptions: set[LicenseException] = set()
    used_clarifications: set[Clarification] = set()
    for pkg in sorted(packages, key=lambda p: (p.project, p.name, p.version)):
        license_expr = pkg.license
        for clar in CLARIFICATIONS:
            if _matches(clar.name, clar.versions, pkg.name, pkg.version):
                used_clarifications.add(clar)
                license_expr = clar.license
        if satisfies(license_expr, ALLOWED):
            continue
        where = f"{pkg.project}: {pkg.name}@{pkg.version} ({license_expr})"
        if pkg.production:
            problems.append(f"{where} is in the production tree; only allowed licences may ship")
            continue
        matching = [e for e in EXCEPTIONS if _matches(e.name, e.versions, pkg.name, pkg.version)]
        if matching and satisfies(license_expr, ALLOWED | {e.license for e in matching}):
            used_exceptions.update(matching)
            continue
        problems.append(f"{where} is not an allowed licence and has no matching exception")
    unused = [f"exception {e.name} {e.versions} ({e.license})" for e in EXCEPTIONS if e not in used_exceptions]
    unused += [f"clarification {c.name} {c.versions}" for c in CLARIFICATIONS if c not in used_clarifications]
    return problems, unused


# --- pnpm and the lockfile -----------------------------------------------------------------


def parse_licenses_json(data: dict) -> dict[tuple[str, str], str]:
    """`pnpm licenses list --json` output -> {(name, version): licence}."""
    result: dict[tuple[str, str], str] = {}
    for group, pkgs in data.items():
        for pkg in pkgs:
            for version in pkg.get("versions", []):
                result[(pkg["name"], version)] = normalize_license(pkg.get("license", group))
    return result


def _split_ref(ref: str) -> tuple[str, str]:
    """'@scope/name@1.2.3(peer@1)' -> ('@scope/name', '1.2.3')."""
    ref = ref.strip().strip("'\"").split("(")[0]
    name, _, version = ref.rpartition("@")
    return name, version


class CheckError(Exception):
    """The check couldn't run reliably (unknown lockfile format, failed registry lookup)."""


# Lockfile formats parse_lockfile understands. pnpm 9, 10 and 11 all write 9.0.
KNOWN_LOCKFILE_VERSIONS = {"9.0"}


def parse_lockfile(text: str) -> tuple[set[tuple[str, str]], dict[tuple[str, str], set[tuple[str, str]]]]:
    """pnpm-lock.yaml (v9) -> (all packages, {package: its direct dependencies}).

    Reads only the `packages:` keys and the `snapshots:` dependency lists, which pnpm writes
    in a fixed two-space layout. Raises CheckError on anything else, so a format change can't
    silently skip packages.
    """
    first = text.split("\n", 1)[0].strip()
    version = first.partition(":")[2].strip().strip("'\"")
    if not first.startswith("lockfileVersion:") or version not in KNOWN_LOCKFILE_VERSIONS:
        raise CheckError(f"unsupported pnpm lockfile format ({first!r}); update parse_lockfile")
    packages: set[tuple[str, str]] = set()
    children: dict[tuple[str, str], set[tuple[str, str]]] = {}
    section = None
    current: tuple[str, str] | None = None
    in_deps = False
    content_lines = 0
    for line in text.splitlines():
        if line and not line.startswith(" "):
            section = line.rstrip(":").strip()
            current = None
            continue
        if section not in ("packages", "snapshots") or not line.strip():
            continue
        content_lines += 1
        indent = len(line) - len(line.lstrip(" "))
        stripped = line.strip()
        if indent == 2:
            key = stripped.split(":", 1)[0] if not stripped.startswith("'") else stripped[1:].split("'", 1)[0]
            current = _split_ref(key)
            if section == "packages":
                packages.add(current)
            else:
                children.setdefault(current, set())
            in_deps = False
        elif section == "snapshots" and indent == 4:
            in_deps = stripped in ("dependencies:", "optionalDependencies:")
        elif section == "snapshots" and indent == 6 and in_deps and current is not None:
            name, _, ref = stripped.partition(": ")
            name = name.strip("'\"")
            ref = ref.strip("'\"")
            if "@" in ref.split("(")[0][1:]:  # alias: `name: real-name@1.2.3`
                children[current].add(_split_ref(ref))
            else:
                children[current].add((name, ref.split("(")[0]))
    if content_lines and not packages:
        raise CheckError("no package entries found in pnpm lockfile's `packages:` section; update parse_lockfile")
    if set(children) != packages:
        raise CheckError(
            "pnpm lockfile's `packages:` and `snapshots:` sections disagree "
            f"({len(packages)} vs {len(children)} entries); update parse_lockfile"
        )
    return packages, children


def registry_license(name: str, version: str, urlopen=urllib.request.urlopen, sleep=time.sleep) -> str:
    """The licence npm's registry records for exactly name@version."""
    url = f"https://registry.npmjs.org/{urllib.parse.quote(name, safe='@')}/{urllib.parse.quote(version)}"
    for attempt in range(4):
        try:
            with urlopen(url, timeout=30) as resp:
                data = json.load(resp)
            if not isinstance(data, dict):
                raise ValueError("not a JSON object")
            return normalize_license(data.get("license") or data.get("licenses"))
        except urllib.error.HTTPError as e:
            if 400 <= e.code < 500 or attempt == 3:
                raise CheckError(f"npm registry lookup of {name}@{version} failed: HTTP {e.code}") from e
        except ValueError as e:  # includes json.JSONDecodeError
            raise CheckError(f"npm registry lookup of {name}@{version} returned bad JSON: {e}") from e
        except OSError as e:
            if attempt == 3:
                raise CheckError(f"npm registry lookup of {name}@{version} failed: {e}") from e
        sleep(2**attempt)
    raise AssertionError("unreachable")


def pnpm_licenses(project: str, prod: bool, run=subprocess.run, which=shutil.which) -> dict:
    # shutil.which finds pnpm.cmd on Windows, which subprocess can't run by bare name.
    pnpm = which("pnpm")
    if pnpm is None:
        raise CheckError("pnpm not found on PATH")
    cmd = [pnpm, "licenses", "list", "--json"]
    if prod:
        cmd.append("--prod")
    if project in IGNORE_WORKSPACE:
        cmd.append("--ignore-workspace")
    # pnpm writes UTF-8; the locale's default (cp1252 on Windows) would garble author names.
    out = run(cmd, cwd=ROOT / project, check=True, capture_output=True, text=True, encoding="utf-8").stdout
    return json.loads(out) if out.strip() else {}


def collect_project(project: str, ships: bool, lookup=registry_license, run=pnpm_licenses) -> list[Package]:
    installed = parse_licenses_json(run(project, False))
    prod = set(parse_licenses_json(run(project, True))) if ships else set()
    lock_packages, children = parse_lockfile((ROOT / project / "pnpm-lock.yaml").read_text())
    unknown = set(installed) - lock_packages
    if unknown:
        raise CheckError(
            f"{project}: installed packages missing from pnpm-lock.yaml, so the lockfile parse "
            f"is incomplete or node_modules is stale: {sorted(unknown)[:5]}"
        )

    missing = sorted(lock_packages - set(installed))
    licenses = dict(installed)
    with ThreadPoolExecutor(max_workers=8) as pool:
        licenses.update(zip(missing, pool.map(lambda key: lookup(*key), missing)))

    # A package pnpm skipped (another platform's binary) is production when a production
    # package depends on it.
    if ships:
        changed = True
        while changed:
            changed = False
            for parent, deps in children.items():
                if parent in prod:
                    for dep in deps:
                        if dep in licenses and dep not in prod:
                            prod.add(dep)
                            changed = True

    return [Package(project, name, version, lic, (name, version) in prod) for (name, version), lic in licenses.items()]


def main(argv: list[str] | None = None, collect=collect_project) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--summary", action="store_true", help="print each project's licences and counts")
    args = parser.parse_args(argv)

    packages: list[Package] = []
    try:
        for project, ships in PROJECTS:
            packages.extend(collect(project, ships))
    except CheckError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1

    if args.summary:
        for project, _ in PROJECTS:
            counts: dict[str, int] = {}
            for pkg in packages:
                if pkg.project == project:
                    counts[pkg.license] = counts.get(pkg.license, 0) + 1
            print(f"{project}: " + ", ".join(f"{lic} {n}" for lic, n in sorted(counts.items(), key=lambda kv: -kv[1])))

    problems, unused = check_packages(packages)
    for problem in problems:
        print(f"error: {problem}", file=sys.stderr)
    for entry in unused:
        print(f"error: {entry} matches no package; remove it here and in docs/LICENSE_COMPLIANCE.md", file=sys.stderr)
    if problems or unused:
        print(
            "See docs/LICENSE_COMPLIANCE.md: a licence outside the allowed list needs an owner decision.",
            file=sys.stderr,
        )
        return 1
    print(f"Licences OK: {len(packages)} npm packages across {len(PROJECTS)} projects.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
