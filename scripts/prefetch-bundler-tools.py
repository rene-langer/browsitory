#!/usr/bin/env python3
"""Download the tools the Tauri bundler fetches during `tauri build`, with retries (#454).

While bundling, the Tauri CLI downloads tools from GitHub release downloads into its tools
folder, unless they are already there:

- Windows (`%LOCALAPPDATA%\\tauri`): WiX 3.14 for the .msi (`WixTools314`) and NSIS 3.13 with
  the `nsis_tauri_utils` plugin for the setup .exe (`NSIS`);
- Linux (`$XDG_CACHE_HOME/tauri`, by default `~/.cache/tauri`): AppRun, linuxdeploy and the
  linuxdeploy AppImage plugin for the AppImage.

It tries each download once, so one HTTP 504 from GitHub fails the whole build after the long
compile. ci.yml's `build-dist` and release.yml's `build-release` cache that folder
(`actions/cache`, keyed on the locked CLI version) and run this script before bundling: it puts
every missing tool in place the way the bundler would, retrying each download, so the bundler
finds them all and downloads nothing. macOS bundling downloads nothing; there it does nothing.

The tool list below is copied from the tauri-bundler of TAURI_CLI_VERSION
(crates/tauri-bundler/src/bundle/{windows/msi,windows/nsis,linux/appimage}). When
frontend/pnpm-lock.yaml locks another CLI version, the script prints a warning and downloads
nothing, so the bundler fetches its own tools (without retries) until the list is updated:
compare those files at the new `tauri-cli-v<version>` tag, then update the list, the hashes and
TAURI_CLI_VERSION (docs/RELEASING.md, "Bundler tools").

Usage:
    prefetch-bundler-tools.py                                # download what is missing
    prefetch-bundler-tools.py --github-output --no-download  # only write cli-version= and
                                                             # tools-dir= to $GITHUB_OUTPUT,
                                                             # for the cache step

Exit status: 0 when every required tool is in place (or the CLI version doesn't match), 1 when a
required download failed after all attempts.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import os
import pathlib
import re
import shutil
import sys
import tempfile
import time
import urllib.request
import zipfile
from dataclasses import dataclass
from typing import Callable

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCKFILE = REPO_ROOT / "frontend" / "pnpm-lock.yaml"

# The @tauri-apps/cli version whose bundler the tool list below was copied from.
TAURI_CLI_VERSION = "2.12.4"

ATTEMPTS = 4
# Seconds to wait before the 2nd, 3rd and 4th attempt.
BACKOFF = (10, 30, 60)
# Seconds without data before an attempt gives up.
TIMEOUT = 120


@dataclass(frozen=True)
class Tool:
    """One download. `dest` is relative to the tools folder."""

    name: str
    url: str
    dest: str
    # (algorithm, hex digest) of the download; None for a download the bundler doesn't pin either.
    digest: tuple[str, str] | None
    # "file": save as `dest` (executable); "zip": extract into `dest`; "nsis-zip": extract into
    # the tools folder and rename the archive's top folder `nsis-3.13` to `dest`.
    kind: str = "file"
    # The bundler goes on without it when its download fails.
    optional: bool = False
    # Files under `dest` the bundler checks for; the tool counts as present when they all exist.
    required: tuple[str, ...] = ()
    # Saved into `dest` afterwards (the NSIS plugin): (relative path, url, sha1).
    extras: tuple[tuple[str, str, str], ...] = ()


WINDOWS_TOOLS = (
    Tool(
        name="WiX 3.14",
        url="https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip",
        dest="WixTools314",
        digest=("sha256", "6ac824e1642d6f7277d0ed7ea09411a508f6116ba6fae0aa5f2c7daa2ff43d31"),
        kind="zip",
        required=(
            "candle.exe",
            "candle.exe.config",
            "darice.cub",
            "light.exe",
            "light.exe.config",
            "wconsole.dll",
            "winterop.dll",
            "wix.dll",
            "WixUIExtension.dll",
            "WixUtilExtension.dll",
        ),
    ),
    Tool(
        name="NSIS 3.13",
        url="https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.13/nsis-3.13.zip",
        dest="NSIS",
        digest=("sha1", "db14f8af2ea346b786c6ae19343f61aaf5e09b8e"),
        kind="nsis-zip",
        required=(
            "makensis.exe",
            "Bin/makensis.exe",
            "Stubs/lzma-x86-unicode",
            "Stubs/lzma_solid-x86-unicode",
            "Plugins/x86-unicode/additional/nsis_tauri_utils.dll",
            "Include/MUI2.nsh",
            "Include/FileFunc.nsh",
            "Include/x64.nsh",
            "Include/nsDialogs.nsh",
            "Include/WinMessages.nsh",
            "Include/Win/COM.nsh",
            "Include/Win/Propkey.nsh",
            "Include/Win/RestartManager.nsh",
        ),
        extras=(
            (
                "Plugins/x86-unicode/additional/nsis_tauri_utils.dll",
                "https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll",
                "75197fee3c6a814fe035788d1c34ead39349b860",
            ),
        ),
    ),
)

# x86_64 only: the shipped Linux builds are x86_64 (release.yml, #386). The bundler pins no hash
# for these; the sha256 values are those of the files at the pinned release URLs.
LINUX_TOOLS = (
    Tool(
        name="AppRun",
        url="https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-x86_64",
        dest="AppRun-x86_64",
        digest=("sha256", "f30140a43a0a59e46db21bdefdf749b9e9f2c6946e92afabbacf98b8ae73fb4f"),
    ),
    Tool(
        name="linuxdeploy",
        url="https://github.com/tauri-apps/binary-releases/releases/download/linuxdeploy-07333c6/linuxdeploy-x86_64.AppImage",
        dest="linuxdeploy-07333c6-x86_64.AppImage",
        digest=("sha256", "36a2d7e274d12e1050d0e9ecfe11d339ed54720b2bec464c286d53f8b07f5c62"),
    ),
    # A rolling `continuous` release, so no hash; the bundler falls back to linuxdeploy's
    # built-in plugin when it is missing.
    Tool(
        name="linuxdeploy AppImage plugin",
        url="https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage",
        dest="linuxdeploy-plugin-appimage.AppImage",
        digest=None,
        optional=True,
    ),
)


def locked_cli_version(lockfile_text: str) -> str:
    """The single @tauri-apps/cli version frontend/pnpm-lock.yaml locks."""
    versions = set(re.findall(r"^  '@tauri-apps/cli@([^']+)':$", lockfile_text, re.MULTILINE))
    if len(versions) != 1:
        raise ValueError(f"expected one locked @tauri-apps/cli version, found {sorted(versions) or 'none'}")
    return versions.pop()


def tools_dir(platform: str, environ: dict[str, str], home: pathlib.Path) -> pathlib.Path | None:
    """The bundler's tools folder (`dirs::cache_dir()/tauri`), or None where it downloads nothing."""
    if platform == "win32":
        return pathlib.Path(environ["LOCALAPPDATA"]) / "tauri"
    if platform.startswith("linux"):
        xdg = environ.get("XDG_CACHE_HOME", "")
        base = pathlib.Path(xdg) if xdg and pathlib.Path(xdg).is_absolute() else home / ".cache"
        return base / "tauri"
    return None


def tools_for(platform: str) -> tuple[Tool, ...]:
    if platform == "win32":
        return WINDOWS_TOOLS
    if platform.startswith("linux"):
        return LINUX_TOOLS
    return ()


def is_present(tool: Tool, root: pathlib.Path) -> bool:
    dest = root / tool.dest
    if not tool.required:
        return dest.is_file()
    return all((dest / path).exists() for path in tool.required)


def verify(data: bytes, digest: tuple[str, str] | None) -> None:
    if digest is None:
        return
    algorithm, expected = digest
    actual = hashlib.new(algorithm, data).hexdigest()
    if actual != expected.lower():
        raise ValueError(f"{algorithm} mismatch: expected {expected}, got {actual}")


def http_get(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "browsitory-ci-prefetch"})
    with urllib.request.urlopen(request, timeout=TIMEOUT) as response:
        return response.read()


def download(
    url: str,
    digest: tuple[str, str] | None,
    get: Callable[[str], bytes] = http_get,
    sleep: Callable[[float], None] = time.sleep,
) -> bytes:
    """Downloads and verifies `url`, trying ATTEMPTS times. Raises the last error."""
    for attempt in range(1, ATTEMPTS + 1):
        try:
            data = get(url)
            verify(data, digest)
            return data
        except Exception as err:  # noqa: BLE001 - every failure is worth another attempt
            print(f"  attempt {attempt}/{ATTEMPTS} failed: {err}", flush=True)
            if attempt == ATTEMPTS:
                raise
            sleep(BACKOFF[attempt - 1])
    raise AssertionError("unreachable")


def write_executable(path: pathlib.Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    partial = path.with_name(path.name + ".partial")
    partial.write_bytes(data)
    partial.chmod(0o755)
    os.replace(partial, path)


def install(tool: Tool, root: pathlib.Path, get: Callable[[str], bytes], sleep: Callable[[float], None]) -> None:
    """Puts `tool` into `root` the way the bundler does."""
    data = download(tool.url, tool.digest, get, sleep)
    dest = root / tool.dest
    if tool.kind == "file":
        write_executable(dest, data)
        return
    extras = [(path, download(url, ("sha1", sha1), get, sleep)) for path, url, sha1 in tool.extras]
    # Extract next to the final folder and move it into place, so an interrupted run never
    # leaves a half-filled folder that the bundler would take as complete.
    root.mkdir(parents=True, exist_ok=True)
    staging = pathlib.Path(tempfile.mkdtemp(prefix=".prefetch-", dir=root))
    try:
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            archive.extractall(staging)
        built = staging if tool.kind == "zip" else staging / "nsis-3.13"
        for path, extra in extras:
            target = built / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(extra)
        if dest.exists():
            shutil.rmtree(dest)
        if tool.kind == "zip":
            os.replace(staging, dest)
        else:
            os.replace(built, dest)
    finally:
        shutil.rmtree(staging, ignore_errors=True)


def prefetch(
    root: pathlib.Path,
    tools: tuple[Tool, ...],
    get: Callable[[str], bytes] = http_get,
    sleep: Callable[[float], None] = time.sleep,
) -> int:
    failed = 0
    for tool in tools:
        if is_present(tool, root):
            print(f"{tool.name}: already in {root / tool.dest}", flush=True)
            continue
        print(f"{tool.name}: downloading {tool.url}", flush=True)
        try:
            install(tool, root, get, sleep)
        except Exception as err:  # noqa: BLE001
            if tool.optional:
                print(f"::warning::{tool.name} could not be downloaded ({err}); the bundler goes on without it")
                continue
            print(f"::error::{tool.name} could not be downloaded after {ATTEMPTS} attempts: {err}")
            failed += 1
            continue
        print(f"{tool.name}: saved to {root / tool.dest}", flush=True)
    return 1 if failed else 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument(
        "--github-output",
        action="store_true",
        help="write cli-version= and tools-dir= to $GITHUB_OUTPUT before downloading",
    )
    parser.add_argument(
        "--no-download",
        action="store_true",
        help="download nothing (with --github-output: only find the folder and the version)",
    )
    args = parser.parse_args(argv)

    version = locked_cli_version(LOCKFILE.read_text(encoding="utf-8"))
    root = tools_dir(sys.platform, dict(os.environ), pathlib.Path.home())
    if args.github_output:
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
            out.write(f"cli-version={version}\n")
            out.write(f"tools-dir={root or ''}\n")
    if root is None:
        print("The bundler downloads no tools on this platform.")
        return 0
    if args.no_download:
        print(f"@tauri-apps/cli {version}, tools folder {root}")
        return 0
    if version != TAURI_CLI_VERSION:
        print(
            f"::warning::frontend/pnpm-lock.yaml locks @tauri-apps/cli {version}, but "
            f"scripts/prefetch-bundler-tools.py lists the tools of {TAURI_CLI_VERSION}: nothing "
            "prefetched, the bundler downloads its own (see docs/RELEASING.md, \"Bundler tools\")"
        )
        return 0
    return prefetch(root, tools_for(sys.platform))


if __name__ == "__main__":
    sys.exit(main())
