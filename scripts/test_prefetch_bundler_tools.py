#!/usr/bin/env python3
"""Tests for prefetch-bundler-tools.py. Temp folders only, the downloads are faked."""

import contextlib
import hashlib
import importlib.util
import io
import pathlib
import shutil
import sys
import tempfile
import unittest
import zipfile

MODULE_PATH = pathlib.Path(__file__).resolve().parent / "prefetch-bundler-tools.py"
spec = importlib.util.spec_from_file_location("prefetch_bundler_tools", MODULE_PATH)
prefetch = importlib.util.module_from_spec(spec)
assert spec.loader is not None
sys.modules[spec.name] = prefetch  # dataclasses look the module up while the class is built
spec.loader.exec_module(prefetch)

LOCKFILE = (pathlib.Path(__file__).resolve().parent.parent / "frontend" / "pnpm-lock.yaml").read_text(
    encoding="utf-8"
)


def zip_bytes(files):
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as archive:
        for name, data in files.items():
            archive.writestr(name, data)
    return buffer.getvalue()


def no_sleep(_seconds):
    pass


class FakeServer:
    """Serves `responses[url]`: bytes, or a list of bytes/exceptions taken one per request."""

    def __init__(self, responses):
        self.responses = {url: list(r) if isinstance(r, list) else [r] for url, r in responses.items()}
        self.requests = []

    def get(self, url):
        self.requests.append(url)
        queue = self.responses[url]
        item = queue.pop(0) if len(queue) > 1 else queue[0]
        if isinstance(item, Exception):
            raise item
        return item


def quietly(function, *args, **kwargs):
    with contextlib.redirect_stdout(io.StringIO()):
        return function(*args, **kwargs)


class LockedVersionTests(unittest.TestCase):
    def test_reads_the_single_locked_version(self):
        text = "packages:\n\n  '@tauri-apps/cli-linux-x64-gnu@2.12.4':\n    x\n\n  '@tauri-apps/cli@2.12.4':\n    y\n"
        self.assertEqual(prefetch.locked_cli_version(text), "2.12.4")

    def test_refuses_no_or_several_versions(self):
        with self.assertRaises(ValueError):
            prefetch.locked_cli_version("packages:\n")
        with self.assertRaises(ValueError):
            prefetch.locked_cli_version("  '@tauri-apps/cli@2.12.4':\n  '@tauri-apps/cli@2.13.0':\n")

    def test_reads_this_repositorys_lockfile(self):
        self.assertRegex(prefetch.locked_cli_version(LOCKFILE), r"^2\.\d+\.\d+$")


class ToolsDirTests(unittest.TestCase):
    def test_windows_uses_local_app_data(self):
        root = prefetch.tools_dir("win32", {"LOCALAPPDATA": "C:/Users/me/AppData/Local"}, pathlib.Path("/h"))
        self.assertEqual(root, pathlib.Path("C:/Users/me/AppData/Local") / "tauri")

    def test_linux_uses_xdg_cache_home_when_absolute(self):
        home = pathlib.Path("/home/me")
        self.assertEqual(prefetch.tools_dir("linux", {}, home), home / ".cache" / "tauri")
        self.assertEqual(prefetch.tools_dir("linux", {"XDG_CACHE_HOME": "/c"}, home), pathlib.Path("/c/tauri"))
        self.assertEqual(prefetch.tools_dir("linux", {"XDG_CACHE_HOME": "rel"}, home), home / ".cache" / "tauri")

    def test_macos_has_none(self):
        self.assertIsNone(prefetch.tools_dir("darwin", {}, pathlib.Path("/h")))
        self.assertEqual(prefetch.tools_for("darwin"), ())


class PrefetchTests(unittest.TestCase):
    def setUp(self):
        self.root = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(lambda: shutil.rmtree(self.root, ignore_errors=True))

    def test_retries_a_failed_download_and_saves_an_executable(self):
        data = b"apprun"
        tool = prefetch.Tool("AppRun", "u/apprun", "AppRun-x86_64", ("sha256", hashlib.sha256(data).hexdigest()))
        server = FakeServer({"u/apprun": [OSError("HTTP Error 504"), data]})
        self.assertEqual(quietly(prefetch.prefetch, self.root, (tool,), server.get, no_sleep), 0)
        self.assertEqual((self.root / "AppRun-x86_64").read_bytes(), data)
        self.assertTrue((self.root / "AppRun-x86_64").stat().st_mode & 0o100)
        self.assertEqual(server.requests, ["u/apprun", "u/apprun"])

    def test_a_wrong_hash_is_a_failed_attempt(self):
        tool = prefetch.Tool("AppRun", "u/apprun", "AppRun-x86_64", ("sha256", "00" * 32))
        server = FakeServer({"u/apprun": b"tampered"})
        self.assertEqual(quietly(prefetch.prefetch, self.root, (tool,), server.get, no_sleep), 1)
        self.assertEqual(len(server.requests), prefetch.ATTEMPTS)
        self.assertFalse((self.root / "AppRun-x86_64").exists())

    def test_an_optional_tool_that_fails_is_only_a_warning(self):
        tool = prefetch.Tool("plugin", "u/plugin", "plugin.AppImage", None, optional=True)
        server = FakeServer({"u/plugin": OSError("HTTP Error 504")})
        self.assertEqual(quietly(prefetch.prefetch, self.root, (tool,), server.get, no_sleep), 0)

    def test_a_present_tool_is_not_downloaded(self):
        (self.root / "AppRun-x86_64").write_bytes(b"cached")
        tool = prefetch.Tool("AppRun", "u/apprun", "AppRun-x86_64", None)
        server = FakeServer({})
        self.assertEqual(quietly(prefetch.prefetch, self.root, (tool,), server.get, no_sleep), 0)
        self.assertEqual(server.requests, [])

    def test_wix_is_extracted_into_its_folder(self):
        archive = zip_bytes({name: b"x" for name in prefetch.WINDOWS_TOOLS[0].required})
        tool = prefetch.Tool(
            "WiX",
            "u/wix",
            "WixTools314",
            ("sha256", hashlib.sha256(archive).hexdigest()),
            kind="zip",
            required=prefetch.WINDOWS_TOOLS[0].required,
        )
        server = FakeServer({"u/wix": archive})
        self.assertEqual(quietly(prefetch.prefetch, self.root, (tool,), server.get, no_sleep), 0)
        self.assertTrue(prefetch.is_present(tool, self.root))
        self.assertEqual(sorted(p.name for p in self.root.iterdir()), ["WixTools314"])

    def test_nsis_is_renamed_and_gets_its_plugin(self):
        nsis = prefetch.WINDOWS_TOOLS[1]
        plugin_path = nsis.extras[0][0]
        files = {f"nsis-3.13/{name}": b"x" for name in nsis.required if name != plugin_path}
        archive = zip_bytes(files)
        plugin = b"plugin"
        tool = prefetch.Tool(
            "NSIS",
            "u/nsis",
            "NSIS",
            ("sha1", hashlib.sha1(archive).hexdigest()),
            kind="nsis-zip",
            required=nsis.required,
            extras=((plugin_path, "u/plugin", hashlib.sha1(plugin).hexdigest()),),
        )
        server = FakeServer({"u/nsis": archive, "u/plugin": plugin})
        self.assertEqual(quietly(prefetch.prefetch, self.root, (tool,), server.get, no_sleep), 0)
        self.assertTrue(prefetch.is_present(tool, self.root))
        self.assertEqual((self.root / "NSIS" / plugin_path).read_bytes(), plugin)
        self.assertEqual(sorted(p.name for p in self.root.iterdir()), ["NSIS"])


if __name__ == "__main__":
    unittest.main()
