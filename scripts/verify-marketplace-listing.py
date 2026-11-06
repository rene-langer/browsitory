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

Network errors, HTTP errors and broken responses count as "not listed yet" and are retried at
the next poll. On HTTP 429 the next poll waits for Retry-After (at most 5 minutes) instead of
the interval, if that is longer.

Exit status: 0 when every target is listed, 1 when some are still missing at the timeout, 2 on
bad arguments.
"""

from __future__ import annotations

import argparse
import http.client
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
ID_RE = re.compile(r"^[a-z0-9][a-z0-9-]*$")
TIMEOUT_SECONDS = 30

# Errors that mean "try again next round", not a crash: network errors and HTTP errors
# (urllib.error.HTTPError is a URLError, which is an OSError), a broken HTTP response such as
# IncompleteRead or BadStatusLine (http.client.HTTPException), and a body that is not JSON.
TRANSIENT_ERRORS = (OSError, http.client.HTTPException, ValueError)

# On HTTP 429: wait as long as Retry-After says, at most this long; without a usable
# Retry-After, wait DEFAULT_THROTTLE_SECONDS.
MAX_RETRY_AFTER_SECONDS = 300
DEFAULT_THROTTLE_SECONDS = 60


class Throttled(Exception):
    """The registry answered 429 Too Many Requests."""

    def __init__(self, retry_after: float):
        super().__init__(f"throttled (429), retry after {int(retry_after)} s")
        self.retry_after = retry_after


def retry_after_seconds(value: str | None) -> float:
    """Seconds from a Retry-After header (delta seconds), capped; a default otherwise."""
    try:
        seconds = float(value) if value is not None else DEFAULT_THROTTLE_SECONDS
    except ValueError:
        seconds = DEFAULT_THROTTLE_SECONDS
    return min(max(seconds, 0.0), MAX_RETRY_AFTER_SECONDS)


def raise_if_throttled(error: urllib.error.HTTPError) -> None:
    if error.code == 429:
        headers = error.headers
        raise Throttled(retry_after_seconds(headers.get("Retry-After") if headers else None))


def http_get_status(url: str) -> int:
    """Status of a GET to url, without reading the body (a .vsix is several MB)."""
    request = urllib.request.Request(url, method="GET")
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
            return response.status
    except urllib.error.HTTPError as error:
        raise_if_throttled(error)
        return error.code


def http_get_json(url: str) -> tuple[int, object]:
    request = urllib.request.Request(url, headers={"Accept": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        raise_if_throttled(error)
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
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        raise_if_throttled(error)
        raise


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


class Round:
    """One polling round. Calling it with a target returns None when the target is listed,
    else a short reason. `retry_after` is set when a registry answered 429 in this round."""

    def __init__(self) -> None:
        self.retry_after: float | None = None

    def throttled(self, error: Throttled) -> str:
        self.retry_after = max(self.retry_after or 0.0, error.retry_after)
        return str(error)

    def __call__(self, target: str) -> str | None:
        raise NotImplementedError


class MarketplaceRound(Round):
    """A single extensionquery, then a download check per validated target."""

    def __init__(
        self,
        publisher: str,
        name: str,
        version: str,
        query: Callable[[str, str], object] | None = None,
        get_status: Callable[[str], int] | None = None,
    ) -> None:
        super().__init__()
        self.publisher, self.name, self.version = publisher, name, version
        self.get_status = get_status or http_get_status
        self.validated: set[str] | None = None
        try:
            answer = (query or query_marketplace)(publisher, name)
            self.validated = validated_marketplace_targets(answer, version)
            self.query_error = ""
        except Throttled as error:
            self.query_error = f"extensionquery {self.throttled(error)}"
        except TRANSIENT_ERRORS as error:
            self.query_error = f"extensionquery failed: {error!r}"

    def __call__(self, target: str) -> str | None:
        if self.validated is None:
            return self.query_error
        if target not in self.validated:
            return "not listed as validated by extensionquery"
        url = MARKETPLACE_DOWNLOAD_URL.format(
            publisher=self.publisher, name=self.name, version=self.version, target=target
        )
        try:
            status = self.get_status(url)
        except Throttled as error:
            return f"download check {self.throttled(error)}"
        except TRANSIENT_ERRORS as error:
            return f"download check failed: {error!r}"
        return None if status == 200 else f"download URL answered {status}"


class OpenVsxRound(Round):
    def __init__(
        self,
        publisher: str,
        name: str,
        version: str,
        get_json: Callable[[str], tuple[int, object]] | None = None,
    ) -> None:
        super().__init__()
        self.publisher, self.name, self.version = publisher, name, version
        self.get_json = get_json or http_get_json

    def __call__(self, target: str) -> str | None:
        url = OPEN_VSX_URL.format(
            publisher=self.publisher, name=self.name, target=target, version=self.version
        )
        try:
            status, data = self.get_json(url)
        except Throttled as error:
            return self.throttled(error)
        except TRANSIENT_ERRORS as error:
            return f"request failed: {error!r}"
        if status != 200:
            return f"answered {status}"
        if open_vsx_listed(data, target, self.version):
            return None
        return "no download for this target"


def wait_for_targets(
    targets: Iterable[str],
    new_round: Callable[[], Callable[[str], "str | None"]],
    timeout: float,
    interval: float,
    log: Callable[[str], None] = print,
    clock: Callable[[], float] | None = None,
    sleep: Callable[[float], None] | None = None,
) -> dict[str, str]:
    """Poll until every target is listed or timeout seconds passed.

    Returns the targets still missing, each with the last reason; empty when all are listed.
    Always polls at least once, even with timeout 0. After a round that was throttled (429),
    waits the Retry-After time (capped) instead of the interval, if that is longer.
    """
    clock = clock or time.monotonic
    sleep = sleep or time.sleep
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
        delay = max(interval, getattr(check, "retry_after", None) or 0.0)
        waiting = ", ".join(f"{t} ({r})" for t, r in sorted(missing.items()))
        log(f"waiting: {waiting}; next check in {int(min(delay, remaining))} s, "
            f"{int(remaining)} s left")
        sleep(min(delay, remaining))


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


def identifier(value: str) -> str:
    if not ID_RE.match(value):
        raise argparse.ArgumentTypeError(f"{value!r} is not a lowercase publisher or name")
    return value


def non_negative_seconds(value: str) -> float:
    seconds = float(value)
    if not seconds >= 0:
        raise argparse.ArgumentTypeError(f"{value!r} is not 0 or more seconds")
    return seconds


def positive_seconds(value: str) -> float:
    seconds = float(value)
    if not seconds > 0:
        raise argparse.ArgumentTypeError(f"{value!r} is not more than 0 seconds")
    return seconds


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Wait until every platform package of a version is public on a registry."
    )
    parser.add_argument("--registry", choices=["marketplace", "open-vsx"], default="marketplace")
    parser.add_argument("--version", required=True, help="extension version, e.g. 0.3.0")
    parser.add_argument("--publisher", type=identifier, default=PUBLISHER)
    parser.add_argument("--name", type=identifier, default=NAME)
    parser.add_argument(
        "--timeout",
        type=non_negative_seconds,
        default=1800,
        help="seconds to keep polling (default 1800)",
    )
    parser.add_argument(
        "--interval",
        type=positive_seconds,
        default=30,
        help="seconds between polls (default 30)",
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

        def new_round() -> Round:
            return MarketplaceRound(args.publisher, args.name, args.version)

    else:
        registry = "Open VSX"

        def new_round() -> Round:
            return OpenVsxRound(args.publisher, args.name, args.version)

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
