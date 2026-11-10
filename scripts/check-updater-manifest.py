#!/usr/bin/env python3
"""Check a draft release's updater manifest (latest.json) before it is published.

Every installed desktop app reads `latest.json` from the latest release to find its update
(docs/RELEASING.md, "Updater manifest"). The three `build-release` jobs of release.yml each
read, rewrite and re-upload it; when two finish at the same moment, one platform's entries can
be lost (#398). This script checks the manifest against the release's assets:

- `version` is the release version;
- every platform in REQUIRED_PLATFORMS is present (others, such as `darwin-universal`, are
  allowed and checked the same way);
- each `url` is the API asset URL
  (`https://api.github.com/repos/<repo>/releases/assets/<id>`, as tauri-action v1 writes it) of
  an uploaded asset of this release, and that asset is a bundle of the platform's kind;
- each `signature` is the content of that asset's `.sig` file.

It reads only local files, so the caller downloads them first (release.yml's
`publish-vsix-release` job does this on the draft):

    gh release download "$tag" -D "$dir" -p latest.json -p '*.sig'
    gh release view "$tag" --json assets > "$dir/assets.json"
    check-updater-manifest.py --version X.Y.Z --assets "$dir/assets.json" "$dir"

With `--pages-copy FILE`, a manifest that passed the checks is also written to FILE with each
`url` rewritten to the public download form,
`https://github.com/<repo>/releases/download/release%2F<version>/<file>`. docs.yml publishes that
copy on the Pages site, the app's second endpoint, so a client that reaches it can still fetch
the bundle when api.github.com is the host that fails (#519). The signatures cover the bundle,
not its URL, so they stay valid.

Exit status: 0 when the manifest is right, 1 when it is not (every problem is printed), 2 on bad
arguments.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys
import urllib.parse

DEFAULT_REPO = "rene-langer/browsitory"

# The platform keys tauri-action v1 writes for the bundles release.yml builds, with the file
# name endings of the bundle each one may point to.
APPIMAGE = (".AppImage",)
MSI = (".msi",)
NSIS = ("-setup.exe",)
APP_TAR_GZ = (".app.tar.gz",)
REQUIRED_PLATFORMS: dict[str, tuple[str, ...]] = {
    "linux-x86_64": APPIMAGE,
    "linux-x86_64-appimage": APPIMAGE,
    "linux-x86_64-deb": (".deb",),
    "linux-x86_64-rpm": (".rpm",),
    "windows-x86_64": MSI + NSIS,
    "windows-x86_64-msi": MSI,
    "windows-x86_64-nsis": NSIS,
    "darwin-aarch64": APP_TAR_GZ,
    "darwin-x86_64": APP_TAR_GZ,
    "darwin-aarch64-app": APP_TAR_GZ,
    "darwin-x86_64-app": APP_TAR_GZ,
}

VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
REPO_RE = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")


def api_url_prefix(repo: str) -> str:
    return f"https://api.github.com/repos/{repo}/releases/assets/"


def manifest_problems(
    manifest: object,
    assets: list[dict],
    signatures: dict[str, str],
    version: str,
    repo: str,
) -> list[str]:
    """Every problem with `manifest` (the parsed latest.json), or [] if there is none.

    `assets` is the release's asset list as `gh release view --json assets` gives it (each a
    dict with at least `name`, `apiUrl` and `state`); `signatures` maps a `.sig` asset's name to
    its content.
    """
    if not isinstance(manifest, dict):
        return ["latest.json is not a JSON object"]

    problems = []
    if manifest.get("version") != version:
        problems.append(f"version is {manifest.get('version')!r}, expected {version!r}")

    platforms = manifest.get("platforms")
    if not isinstance(platforms, dict):
        return problems + ["latest.json has no `platforms` object"]

    for platform in REQUIRED_PLATFORMS:
        if platform not in platforms:
            problems.append(f"{platform}: missing")

    by_api_url = {
        a.get("apiUrl"): a for a in assets if isinstance(a, dict) and a.get("state") == "uploaded"
    }
    prefix = api_url_prefix(repo)
    for platform, entry in sorted(platforms.items()):
        if not isinstance(entry, dict):
            problems.append(f"{platform}: entry is not an object")
            continue
        url = entry.get("url")
        signature = entry.get("signature")
        if not isinstance(url, str) or not url.startswith(prefix):
            problems.append(f"{platform}: url {url!r} is not an API asset URL ({prefix}<id>)")
            continue
        asset = by_api_url.get(url)
        if asset is None:
            problems.append(f"{platform}: url {url} is not an uploaded asset of this release")
            continue
        name = asset.get("name", "")
        endings = REQUIRED_PLATFORMS.get(platform)
        if endings is not None and not name.endswith(endings):
            problems.append(
                f"{platform}: points to {name}, expected a file ending in {' or '.join(endings)}"
            )
        if not isinstance(signature, str) or not signature.strip():
            problems.append(f"{platform}: no signature")
            continue
        expected = signatures.get(f"{name}.sig")
        if expected is None:
            problems.append(f"{platform}: release has no {name}.sig")
        elif signature.strip() != expected.strip():
            problems.append(f"{platform}: signature differs from {name}.sig")
    return problems


def pages_copy(manifest: dict, assets: list[dict], version: str, repo: str) -> dict:
    """`manifest` with every bundle `url` as github.com/<repo>/releases/download/<tag>/<file>.

    Only for a manifest that passed `manifest_problems`, so every url names an uploaded asset.
    """
    names = {a["apiUrl"]: a["name"] for a in assets if a.get("state") == "uploaded"}
    base = f"https://github.com/{repo}/releases/download/release%2F{version}/"
    copy = json.loads(json.dumps(manifest))
    for entry in copy["platforms"].values():
        entry["url"] = base + urllib.parse.quote(names[entry["url"]])
    return copy


def load_signatures(directory: pathlib.Path) -> dict[str, str]:
    return {p.name: p.read_text(encoding="utf-8") for p in directory.glob("*.sig") if p.is_file()}


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--version", required=True, help="release version, MAJOR.MINOR.PATCH")
    parser.add_argument(
        "--assets",
        required=True,
        type=pathlib.Path,
        help="the output of `gh release view <tag> --json assets`",
    )
    parser.add_argument(
        "--repo", default=DEFAULT_REPO, help=f"owner/name of the repository (default {DEFAULT_REPO})"
    )
    parser.add_argument(
        "--pages-copy",
        type=pathlib.Path,
        metavar="FILE",
        help="also write the checked manifest to FILE with github.com download URLs",
    )
    parser.add_argument(
        "directory",
        type=pathlib.Path,
        help="directory holding the release's latest.json and .sig files",
    )
    args = parser.parse_args(argv)
    if not VERSION_RE.match(args.version):
        parser.error(f"--version {args.version!r} is not MAJOR.MINOR.PATCH")
    if not REPO_RE.match(args.repo):
        parser.error(f"--repo {args.repo!r} is not owner/name")
    return args


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    try:
        manifest = json.loads((args.directory / "latest.json").read_text(encoding="utf-8"))
        assets_doc = json.loads(args.assets.read_text(encoding="utf-8"))
        signatures = load_signatures(args.directory)
    except (OSError, ValueError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 1
    assets = assets_doc.get("assets") if isinstance(assets_doc, dict) else assets_doc
    if not isinstance(assets, list):
        print(f"error: {args.assets} has no asset list", file=sys.stderr)
        return 1

    problems = manifest_problems(manifest, assets, signatures, args.version, args.repo)
    if problems:
        print(f"latest.json of {args.version} is not right:", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        return 1
    if args.pages_copy is not None:
        copy = pages_copy(manifest, assets, args.version, args.repo)
        try:
            args.pages_copy.write_text(json.dumps(copy, indent=2) + "\n", encoding="utf-8")
        except OSError as err:
            print(f"error: {err}", file=sys.stderr)
            return 1
    print(
        f"latest.json of {args.version}: {len(manifest['platforms'])} platforms, "
        "each linked to an asset of this release and signed with its .sig"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
