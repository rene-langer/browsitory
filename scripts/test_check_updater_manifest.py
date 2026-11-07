#!/usr/bin/env python3
"""Tests for check-updater-manifest.py. Files only, no network."""

import contextlib
import importlib.util
import io
import json
import pathlib
import tempfile
import unittest

MODULE_PATH = pathlib.Path(__file__).resolve().parent / "check-updater-manifest.py"
spec = importlib.util.spec_from_file_location("check_updater_manifest", MODULE_PATH)
check = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(check)

REPO = "rene-langer/browsitory"
VERSION = "0.4.0"

# Bundle file names as tauri-action v1 uploads them for 0.4.0.
BUNDLES = {
    "appimage": "Browsitory_0.4.0_amd64.AppImage",
    "deb": "Browsitory_0.4.0_amd64.deb",
    "rpm": "Browsitory-0.4.0-1.x86_64.rpm",
    "msi": "Browsitory_0.4.0_x64_en-US.msi",
    "nsis": "Browsitory_0.4.0_x64-setup.exe",
    "app": "Browsitory_0.4.0_universal.app.tar.gz",
}
PLATFORM_BUNDLE = {
    "linux-x86_64": "appimage",
    "linux-x86_64-appimage": "appimage",
    "linux-x86_64-deb": "deb",
    "linux-x86_64-rpm": "rpm",
    "windows-x86_64": "msi",
    "windows-x86_64-msi": "msi",
    "windows-x86_64-nsis": "nsis",
    "darwin-aarch64": "app",
    "darwin-x86_64": "app",
    "darwin-aarch64-app": "app",
    "darwin-x86_64-app": "app",
    "darwin-universal": "app",
    "darwin-universal-app": "app",
}


def api_url(asset_id):
    return f"https://api.github.com/repos/{REPO}/releases/assets/{asset_id}"


def release():
    """(assets, signatures, manifest) of a correct 0.4.0 draft."""
    assets = []
    signatures = {}
    next_id = 1000
    for name in [*BUNDLES.values(), "Browsitory_0.4.0_universal.dmg", "latest.json"]:
        assets.append({"name": name, "apiUrl": api_url(next_id), "state": "uploaded"})
        next_id += 1
    for name in BUNDLES.values():
        sig = f"{name}.sig"
        assets.append({"name": sig, "apiUrl": api_url(next_id), "state": "uploaded"})
        signatures[sig] = f"signature-of-{name}\n"
        next_id += 1
    url_of = {a["name"]: a["apiUrl"] for a in assets}
    manifest = {
        "version": VERSION,
        "notes": "notes",
        "pub_date": "2026-11-07T12:00:00.000Z",
        "platforms": {
            platform: {
                "url": url_of[BUNDLES[kind]],
                "signature": f"signature-of-{BUNDLES[kind]}",
            }
            for platform, kind in PLATFORM_BUNDLE.items()
        },
    }
    return assets, signatures, manifest


def problems(manifest, assets, signatures, version=VERSION):
    return check.manifest_problems(manifest, assets, signatures, version, REPO)


class ManifestProblemsTests(unittest.TestCase):
    def test_a_correct_manifest_has_no_problems(self):
        assets, signatures, manifest = release()
        self.assertEqual(problems(manifest, assets, signatures), [])

    def test_extra_platforms_are_optional(self):
        assets, signatures, manifest = release()
        del manifest["platforms"]["darwin-universal"]
        del manifest["platforms"]["darwin-universal-app"]
        self.assertEqual(problems(manifest, assets, signatures), [])

    def test_wrong_version(self):
        assets, signatures, manifest = release()
        self.assertEqual(
            problems(manifest, assets, signatures, version="0.4.1"),
            ["version is '0.4.0', expected '0.4.1'"],
        )

    def test_a_platform_lost_in_the_race_is_reported(self):
        assets, signatures, manifest = release()
        for platform in ["windows-x86_64", "windows-x86_64-msi", "windows-x86_64-nsis"]:
            del manifest["platforms"][platform]
        self.assertEqual(
            problems(manifest, assets, signatures),
            [
                "windows-x86_64: missing",
                "windows-x86_64-msi: missing",
                "windows-x86_64-nsis: missing",
            ],
        )

    def test_a_browser_url_is_not_an_api_asset_url(self):
        assets, signatures, manifest = release()
        manifest["platforms"]["linux-x86_64-deb"]["url"] = (
            "https://github.com/rene-langer/browsitory/releases/download/"
            "release%2F0.4.0/Browsitory_0.4.0_amd64.deb"
        )
        [problem] = problems(manifest, assets, signatures)
        self.assertIn("linux-x86_64-deb: url", problem)
        self.assertIn("is not an API asset URL", problem)

    def test_an_asset_of_another_repository_is_rejected(self):
        assets, signatures, manifest = release()
        manifest["platforms"]["linux-x86_64-deb"]["url"] = (
            "https://api.github.com/repos/someone/else/releases/assets/1002"
        )
        [problem] = problems(manifest, assets, signatures)
        self.assertIn("is not an API asset URL", problem)

    def test_a_url_that_is_not_an_asset_of_this_release(self):
        assets, signatures, manifest = release()
        manifest["platforms"]["darwin-aarch64"]["url"] = api_url(1)
        self.assertEqual(
            problems(manifest, assets, signatures),
            [f"darwin-aarch64: url {api_url(1)} is not an uploaded asset of this release"],
        )

    def test_an_asset_still_uploading_does_not_count(self):
        assets, signatures, manifest = release()
        for asset in assets:
            if asset["name"] == BUNDLES["rpm"]:
                asset["state"] = "starter"
        [problem] = problems(manifest, assets, signatures)
        self.assertIn("linux-x86_64-rpm: url", problem)
        self.assertIn("is not an uploaded asset", problem)

    def test_a_url_pointing_to_the_wrong_kind_of_bundle(self):
        assets, signatures, manifest = release()
        manifest["platforms"]["windows-x86_64-msi"] = dict(
            manifest["platforms"]["windows-x86_64-nsis"]
        )
        self.assertEqual(
            problems(manifest, assets, signatures),
            [
                f"windows-x86_64-msi: points to {BUNDLES['nsis']}, "
                "expected a file ending in .msi"
            ],
        )

    def test_windows_x86_64_may_point_to_either_installer(self):
        assets, signatures, manifest = release()
        manifest["platforms"]["windows-x86_64"] = dict(
            manifest["platforms"]["windows-x86_64-nsis"]
        )
        self.assertEqual(problems(manifest, assets, signatures), [])

    def test_a_missing_signature(self):
        assets, signatures, manifest = release()
        manifest["platforms"]["linux-x86_64-rpm"]["signature"] = ""
        self.assertEqual(
            problems(manifest, assets, signatures), ["linux-x86_64-rpm: no signature"]
        )

    def test_a_signature_that_differs_from_the_sig_file(self):
        assets, signatures, manifest = release()
        manifest["platforms"]["darwin-x86_64-app"]["signature"] = "something else"
        self.assertEqual(
            problems(manifest, assets, signatures),
            [f"darwin-x86_64-app: signature differs from {BUNDLES['app']}.sig"],
        )

    def test_a_bundle_without_a_sig_file(self):
        assets, signatures, manifest = release()
        del signatures[f"{BUNDLES['deb']}.sig"]
        self.assertEqual(
            problems(manifest, assets, signatures),
            [f"linux-x86_64-deb: release has no {BUNDLES['deb']}.sig"],
        )

    def test_not_an_object(self):
        assets, signatures, _ = release()
        self.assertEqual(problems([], assets, signatures), ["latest.json is not a JSON object"])

    def test_no_platforms(self):
        assets, signatures, _ = release()
        self.assertEqual(
            problems({"version": VERSION}, assets, signatures),
            ["latest.json has no `platforms` object"],
        )

    def test_an_entry_that_is_not_an_object(self):
        assets, signatures, manifest = release()
        manifest["platforms"]["darwin-universal"] = "oops"
        self.assertEqual(
            problems(manifest, assets, signatures),
            ["darwin-universal: entry is not an object"],
        )


class MainTests(unittest.TestCase):
    def run_main(self, argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            try:
                status = check.main(argv)
            except SystemExit as exit_:
                status = exit_.code
        return status, out.getvalue(), err.getvalue()

    def write_release(self, directory, manifest, assets, signatures):
        (directory / "latest.json").write_text(json.dumps(manifest), encoding="utf-8")
        for name, content in signatures.items():
            (directory / name).write_text(content, encoding="utf-8")
        # The shape of `gh release view <tag> --json assets`.
        assets_file = directory / "assets.json"
        assets_file.write_text(json.dumps({"assets": assets}), encoding="utf-8")
        return assets_file

    def test_passes_on_a_correct_release(self):
        assets, signatures, manifest = release()
        with tempfile.TemporaryDirectory() as tmp:
            d = pathlib.Path(tmp)
            assets_file = self.write_release(d, manifest, assets, signatures)
            status, out, err = self.run_main(
                ["--version", VERSION, "--assets", str(assets_file), str(d)]
            )
        self.assertEqual(status, 0, err)
        self.assertIn("13 platforms", out)

    def test_fails_and_lists_every_problem(self):
        assets, signatures, manifest = release()
        del manifest["platforms"]["linux-x86_64-rpm"]
        manifest["platforms"]["darwin-aarch64"]["signature"] = "wrong"
        with tempfile.TemporaryDirectory() as tmp:
            d = pathlib.Path(tmp)
            assets_file = self.write_release(d, manifest, assets, signatures)
            status, _, err = self.run_main(
                ["--version", VERSION, "--assets", str(assets_file), str(d)]
            )
        self.assertEqual(status, 1)
        self.assertIn("linux-x86_64-rpm: missing", err)
        self.assertIn("darwin-aarch64: signature differs", err)

    def test_fails_without_latest_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = pathlib.Path(tmp)
            assets_file = d / "assets.json"
            assets_file.write_text('{"assets": []}', encoding="utf-8")
            status, _, err = self.run_main(
                ["--version", VERSION, "--assets", str(assets_file), str(d)]
            )
        self.assertEqual(status, 1)
        self.assertIn("latest.json", err)

    def test_rejects_a_bad_version_argument(self):
        status, _, _ = self.run_main(["--version", "v0.4.0", "--assets", "a.json", "."])
        self.assertEqual(status, 2)

    def test_rejects_a_bad_repo_argument(self):
        status, _, _ = self.run_main(
            ["--version", VERSION, "--repo", "no-slash", "--assets", "a.json", "."]
        )
        self.assertEqual(status, 2)


if __name__ == "__main__":
    unittest.main()
