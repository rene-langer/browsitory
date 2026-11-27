#!/usr/bin/env python3
"""Tests for ci-apt-install.sh, with `sudo` and `apt-get` faked and a hand-made /etc/apt.

The fake apt-get reads the sources the way apt would (`Dir::Etc::SourceParts` if the command
names it, else the folder's default `sources.list.d`) and, like the runner on 2026-11-27
(#1211), fails `update` with a 403 while one of them is packages.microsoft.com.
"""

import os
import pathlib
import subprocess
import tempfile
import textwrap
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parent / "ci-apt-install.sh"

UBUNTU_SOURCES = textwrap.dedent(
    """\
    Types: deb
    URIs: mirror+file:/etc/apt/apt-mirrors.txt
    Suites: resolute resolute-updates resolute-backports
    Components: main universe restricted multiverse
    Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
    """
)
MICROSOFT_LIST = "deb [arch=amd64] https://packages.microsoft.com/ubuntu/26.04/prod resolute main\n"

FAKE_SUDO = '#!/bin/sh\nexec "$@"\n'

FAKE_APT_GET = textwrap.dedent(
    """\
    #!/bin/bash
    # Logs each call: its arguments, then the source files it would read.
    parts="$CI_APT_ETC/sources.list.d"
    command=
    for arg in "$@"; do
      case "$arg" in
        Dir::Etc::SourceParts=*) parts="${arg#Dir::Etc::SourceParts=}" ;;
        update|install) [ -z "$command" ] && command="$arg" ;;
      esac
    done
    {
      echo "call: $*"
      echo "sources: $(cd "$parts" 2>/dev/null && ls -1 | sort | tr '\\n' ' ')"
    } >> "$APT_LOG"
    if [ "$command" = update ] && grep -qs packages.microsoft.com "$parts"/*; then
      echo "E: Failed to fetch https://packages.microsoft.com/ubuntu/26.04/prod/dists/resolute/InRelease  403  Forbidden"
      exit 100
    fi
    exit 0
    """
)


class CiAptInstallTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.tmp.name)
        self.etc = root / "etc-apt"
        (self.etc / "sources.list.d").mkdir(parents=True)
        (self.etc / "sources.list").write_text("# Ubuntu sources have moved to sources.list.d\n")
        (self.etc / "apt-mirrors.txt").write_text(
            "http://azure.archive.ubuntu.com/ubuntu/\tpriority:1\n"
            "https://archive.ubuntu.com/ubuntu/\tpriority:2\n"
        )
        self.bin = root / "bin"
        self.bin.mkdir()
        for name, text in (("sudo", FAKE_SUDO), ("apt-get", FAKE_APT_GET)):
            path = self.bin / name
            path.write_text(text)
            path.chmod(0o755)
        self.log = root / "apt.log"

    def tearDown(self):
        self.tmp.cleanup()

    def source(self, name, text):
        (self.etc / "sources.list.d" / name).write_text(text)

    def run_script(self, *packages):
        env = dict(os.environ)
        env.update(
            PATH=f"{self.bin}{os.pathsep}{env['PATH']}",
            CI_APT_ETC=str(self.etc),
            APT_LOG=str(self.log),
        )
        return subprocess.run(
            ["bash", str(SCRIPT), *packages],
            env=env,
            capture_output=True,
            text=True,
            timeout=120,
        )

    def calls(self):
        """(arguments, sources) of each apt-get call."""
        lines = self.log.read_text().splitlines() if self.log.exists() else []
        return [
            (lines[i].removeprefix("call: "), lines[i + 1].removeprefix("sources:").split())
            for i in range(0, len(lines), 2)
        ]

    def test_the_runners_microsoft_source_is_left_out(self):
        self.source("ubuntu.sources", UBUNTU_SOURCES)
        self.source("microsoft-prod.list", MICROSOFT_LIST)
        result = self.run_script("libdbus-1-dev", "pkg-config")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.calls()
        # update, download, install: each reads the Ubuntu sources only.
        self.assertEqual(len(calls), 3, calls)
        for arguments, sources in calls:
            self.assertEqual(sources, ["ubuntu.sources"], arguments)
        self.assertIn("update", calls[0][0])
        self.assertIn("--download-only libdbus-1-dev pkg-config", calls[1][0])
        self.assertIn("--no-download libdbus-1-dev pkg-config", calls[2][0])
        self.assertIn("microsoft-prod.list", result.stdout)

    def test_a_source_naming_an_ubuntu_archive_host_is_kept(self):
        self.source("ubuntu.list", "deb http://archive.ubuntu.com/ubuntu resolute main\n")
        self.source("security.list", "deb http://security.ubuntu.com/ubuntu resolute-security main\n")
        self.source("ports.sources", "Types: deb\nURIs: http://ports.ubuntu.com/ubuntu-ports\n")
        self.source("microsoft-prod.list", MICROSOFT_LIST)
        result = self.run_script("rpm")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for arguments, sources in self.calls():
            self.assertEqual(sources, ["ports.sources", "security.list", "ubuntu.list"], arguments)

    def test_without_ubuntu_files_in_the_folder_sources_list_alone_is_read(self):
        # Older images keep the Ubuntu sources in sources.list itself, which apt still reads.
        self.source("microsoft-prod.list", MICROSOFT_LIST)
        result = self.run_script("xvfb")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.calls()
        self.assertEqual(len(calls), 3, calls)
        for arguments, sources in calls:
            self.assertEqual(sources, [], arguments)


if __name__ == "__main__":
    unittest.main()
