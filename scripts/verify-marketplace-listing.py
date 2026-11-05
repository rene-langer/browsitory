#!/usr/bin/env python3
"""Wait until every platform package of an extension version is public on a registry.

The Visual Studio Marketplace can accept a `vsce publish` and still drop the package (#344):
in the 0.3.0 release, three of four platform packages uploaded while the first one was being
validated never appeared, although vsce printed "Published" for each. This script polls the
public gallery until every given target is listed and downloadable, and fails with the list of
missing targets after a timeout. It only sends read-only GET/POST queries to public endpoints;
it needs no token.

    verify-marketplace-listing.py --version 0.3.0 darwin-arm64 darwin-x64 linux-x64 win32-x64
    verify-marketplace-listing.py --registry open-vsx --version 0.3.0 linux-x64
    verify-marketplace-listing.py --packages-dir release-vsix --version 0.3.0

A target counts as listed:

- Marketplace: the `extensionquery` API lists the version for that target with the
  `validated` flag (the Marketplace's own validation result), and the public download URL
  (`.../vsextensions/<name>/<version>/vspackage?targetPlatform=<target>`) answers 200. The
  download URL answers 404 for a missing target; HEAD is not supported there (405), so the
  check sends a GET and closes the response without reading the package.
- Open VSX: `https://open-vsx.org/api/<namespace>/<name>/<target>/<version>` answers 200 with
  that target and version and a download link.

Pre-release versions are listed like stable ones on both registries.

Exit status: 0 when every target is listed, 1 when some are still missing at the timeout, 2 on
bad arguments.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys
import time
import urllib.error
import urllib.request
from typing import Callable, Iterable

PUBLISHER = "browsitory"
NAME = "browsitory"

MARKETPLACE_QUERY_URL = "https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery"
MARKETPLACE_DOWNLOAD_URL = (
    "https://marketplace.visualstudio.com/_apis/public/gallery/publishers/{publisher}"
    "/vsextensions/{name}/{version}/vspackage?targetPlatform={target}"
)
OPEN_VSX_URL = "https://open-vsx.org/api/{publisher}/{name}/{target}/{version}"

# extensionquery: filterType 7 = extension id ("publisher.name"); flags 0x1 = include all
# versions (one entry per version and target), 0x10 = include version properties.
QUERY_FLAGS = 0x1 | 0x10

TARGET_RE = re.compile(r"^[a-z0-9]+-[a-z0-9]+$")
VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
TIMEOUT_SECONDS = 30

# A check returns None when the target is listed, else a short reason it is not (yet).
Check = Callable[[str], "str | None"]


def http_get_status(url: str) -> int:
    """Status of a GET to url, without reading the body (a .vsix is several MB)."""
    request = urllib.request.Request(url, method="GET")
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
            return response.status
    except urllib.error.HTTPError as error:
        return error.code


def http_get_json(url: str) -> tuple[int, object]:
    request = urllib.request.Request(url, headers={"Accept": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, None


def query_marketplace(publisher: str, name: str) -> object:
    body = json.dumps(
        {
            "filters": [{"criteria": [{"filterType": 7, "value": f"{publisher}.{name}"}]}],
            "flags": QUERY_FLAGS,
        }
    ).encode()
    request = urllib.request.Request(
        MARKETPLACE_QUERY_URL,
        data=body,
        headers={
            "Content-Type": "application/json",
            "Accept": "application/json;api-version=7.2-preview.1",
        },
    )
    with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
        return json.load(response)


def validated_marketplace_targets(query_result: object, version: str) -> set[str]:
    """Targets the extensionquery result lists for version with the `validated` flag."""
    targets: set[str] = set()
    if not isinstance(query_result, dict):
        return targets
    for result in query_result.get("results") or []:
        for extension in result.get("extensions") or []:
            for entry in extension.get("versions") or []:
                if entry.get("version") != version:
                    continue
                flags = str(entry.get("flags") or "")
                if "validated" not in [flag.strip() for flag in flags.split(",")]:
                    continue
                # A package without a target is universal; it would serve every platform.
                targets.add(entry.get("targetPlatform") or "universal")
    return targets


def open_vsx_listed(data: object, target: str, version: str) -> bool:
    if not isinstance(data, dict) or data.get("error"):
        return False
    files = data.get("files")
    return (
        data.get("version") == version
        and data.get("targetPlatform") == target
        and isinstance(files, dict)
        and bool(files.get("download"))
    )


def marketplace_round(
    publisher: str,
    name: str,
    version: str,
    query: Callable[[str, str], object] | None = None,
    get_status: Callable[[str], int] | None = None,
) -> Check:
    """One polling round: a single extensionquery, then a download check per validated target."""
    query = query or query_marketplace
    get_status = get_status or http_get_status
    try:
        validated: set[str] | None = validated_marketplace_targets(query(publisher, name), version)
        query_error = ""
    except (urllib.error.URLError, OSError, ValueError) as error:
        validated, query_error = None, f"extensionquery failed: {error}"

    def check(target: str) -> str | None:
        if validated is None:
            return query_error
        if target not in validated:
            return "not listed as validated by extensionquery"
        url = MARKETPLACE_DOWNLOAD_URL.format(
            publisher=publisher, name=name, version=version, target=target
        )
        try:
            status = get_status(url)
        except (urllib.error.URLError, OSError) as error:
            return f"download check failed: {error}"
        return None if status == 200 else f"download URL answered {status}"

    return check


def open_vsx_round(
    publisher: str,
    name: str,
    version: str,
    get_json: Callable[[str], tuple[int, object]] | None = None,
) -> Check:
    get_json = get_json or http_get_json

    def check(target: str) -> str | None:
        url = OPEN_VSX_URL.format(publisher=publisher, name=name, target=target, version=version)
        try:
            status, data = get_json(url)
        except (urllib.error.URLError, OSError, ValueError) as error:
            return f"request failed: {error}"
        if status != 200:
            return f"answered {status}"
        return None if open_vsx_listed(data, target, version) else "no download for this target"

    return check


def wait_for_targets(
    targets: Iterable[str],
    new_round: Callable[[], Check],
    timeout: float,
    interval: float,
    log: Callable[[str], None] = print,
    clock: Callable[[], float] = time.monotonic,
    sleep: Callable[[float], None] = time.sleep,
) -> dict[str, str]:
    """Poll until every target is listed or timeout seconds passed.

    Returns the targets still missing, each with the last reason; empty when all are listed.
    Always polls at least once, even with timeout 0.
    """
    missing = {target: "not checked" for target in targets}
    deadline = clock() + timeout
    while True:
        check = new_round()
        for target in list(missing):
            reason = check(target)
            if reason is None:
                log(f"listed: {target}")
                del missing[target]
            else:
                missing[target] = reason
        if not missing:
            return missing
        remaining = deadline - clock()
        if remaining <= 0:
            return missing
        waiting = ", ".join(f"{t} ({r})" for t, r in sorted(missing.items()))
        log(f"waiting: {waiting}; {int(remaining)} s left")
        sleep(min(interval, remaining))


def targets_from_packages(directory: pathlib.Path, name: str, version: str) -> list[str]:
    """Targets of the <name>-<version>-<target>.vsix files in directory (build-vsix-release)."""
    prefix = f"{name}-{version}-"
    targets = []
    for package in sorted(directory.glob("*.vsix")):
        stem = package.stem
        if not stem.startswith(prefix):
            raise ValueError(f"{package.name} is not named {prefix}<target>.vsix")
        targets.append(stem[len(prefix) :])
    if not targets:
        raise ValueError(f"no .vsix packages in {directory}")
    return targets


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Wait until every platform package of a version is public on a registry."
    )
    parser.add_argument("--registry", choices=["marketplace", "open-vsx"], default="marketplace")
    parser.add_argument("--version", required=True, help="extension version, e.g. 0.3.0")
    parser.add_argument("--publisher", default=PUBLISHER)
    parser.add_argument("--name", default=NAME)
    parser.add_argument(
        "--timeout", type=float, default=1800, help="seconds to keep polling (default 1800)"
    )
    parser.add_argument(
        "--interval", type=float, default=30, help="seconds between polls (default 30)"
    )
    parser.add_argument(
        "--packages-dir",
        type=pathlib.Path,
        help="take the targets from the <name>-<version>-<target>.vsix files in this folder",
    )
    parser.add_argument("targets", nargs="*", help="platform targets, e.g. linux-x64")
    args = parser.parse_args(argv)

    targets = list(args.targets)
    if args.packages_dir is not None:
        try:
            targets += targets_from_packages(args.packages_dir, args.name, args.version)
        except ValueError as error:
            parser.error(str(error))
    targets = list(dict.fromkeys(targets))
    if not targets:
        parser.error("give at least one target or --packages-dir")
    if not VERSION_RE.match(args.version):
        parser.error(f"version {args.version!r} is not MAJOR.MINOR.PATCH")
    for target in targets:
        if not TARGET_RE.match(target):
            parser.error(f"target {target!r} is not of the form os-arch, e.g. linux-x64")

    if args.registry == "marketplace":
        registry = "the Visual Studio Marketplace"

        def new_round() -> Check:
            return marketplace_round(args.publisher, args.name, args.version)

    else:
        registry = "Open VSX"

        def new_round() -> Check:
            return open_vsx_round(args.publisher, args.name, args.version)

    print(
        f"Checking {args.publisher}.{args.name} {args.version} on {registry} for "
        f"{', '.join(targets)} (timeout {int(args.timeout)} s)",
        flush=True,
    )
    missing = wait_for_targets(
        targets,
        new_round,
        timeout=args.timeout,
        interval=args.interval,
        log=lambda line: print(line, flush=True),
    )
    if missing:
        details = "; ".join(f"{t}: {r}" for t, r in sorted(missing.items()))
        print(
            f"::error::Missing on {registry} after {int(args.timeout)} s: {args.version} for "
            f"{', '.join(sorted(missing))} ({details}). "
            "Re-run this job; --skip-duplicate re-uploads only the missing targets."
        )
        return 1
    print(f"All {len(targets)} targets of {args.version} are listed on {registry}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
