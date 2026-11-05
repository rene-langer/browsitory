#!/usr/bin/env python3
"""Tests for verify-marketplace-listing.py. No network: the HTTP calls are replaced by fakes."""

import contextlib
import importlib.util
import io
import pathlib
import tempfile
import unittest
import urllib.error
from unittest import mock

MODULE_PATH = pathlib.Path(__file__).resolve().parent / "verify-marketplace-listing.py"
spec = importlib.util.spec_from_file_location("verify_marketplace_listing", MODULE_PATH)
verify = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(verify)


def query_result(*versions):
    """An extensionquery answer listing (version, target, flags) entries."""
    return {
        "results": [
            {
                "extensions": [
                    {
                        "flags": "validated, public",
                        "versions": [
                            {"version": v, "targetPlatform": t, "flags": f}
                            for v, t, f in versions
                        ],
                    }
                ]
            }
        ]
    }


class FakeClock:
    def __init__(self):
        self.now = 0.0
        self.sleeps = []

    def clock(self):
        return self.now

    def sleep(self, seconds):
        self.sleeps.append(seconds)
        self.now += seconds


class ValidatedTargetsTests(unittest.TestCase):
    def test_only_validated_entries_of_the_version_count(self):
        result = query_result(
            ("0.3.0", "darwin-arm64", "validated"),
            ("0.3.0", "linux-x64", "none"),
            ("0.2.0", "win32-x64", "validated"),
        )
        self.assertEqual(
            verify.validated_marketplace_targets(result, "0.3.0"), {"darwin-arm64"}
        )

    def test_flags_may_list_several_values(self):
        result = query_result(("0.3.0", "linux-x64", "validated, unpublished"))
        self.assertEqual(verify.validated_marketplace_targets(result, "0.3.0"), {"linux-x64"})

    def test_flag_names_match_whole_words(self):
        result = query_result(("0.3.0", "linux-x64", "notvalidated"))
        self.assertEqual(verify.validated_marketplace_targets(result, "0.3.0"), set())

    def test_unknown_or_empty_answers_list_nothing(self):
        for answer in (None, [], {}, {"results": []}, {"results": [{"extensions": []}]}):
            with self.subTest(answer=answer):
                self.assertEqual(verify.validated_marketplace_targets(answer, "0.3.0"), set())


class MarketplaceRoundTests(unittest.TestCase):
    def test_validated_target_still_needs_a_downloadable_package(self):
        urls = []

        def get_status(url):
            urls.append(url)
            return 404 if "linux-x64" in url else 200

        check = verify.marketplace_round(
            "browsitory",
            "browsitory",
            "0.3.0",
            query=lambda p, n: query_result(
                ("0.3.0", "darwin-arm64", "validated"), ("0.3.0", "linux-x64", "validated")
            ),
            get_status=get_status,
        )
        self.assertIsNone(check("darwin-arm64"))
        self.assertEqual(check("linux-x64"), "download URL answered 404")
        self.assertEqual(check("win32-x64"), "not listed as validated by extensionquery")
        self.assertEqual(len(urls), 2, "no download check for a target that is not validated")
        self.assertIn(
            "/publishers/browsitory/vsextensions/browsitory/0.3.0/vspackage"
            "?targetPlatform=darwin-arm64",
            urls[0],
        )

    def test_queries_once_per_round(self):
        calls = []

        def query(publisher, name):
            calls.append((publisher, name))
            return query_result(("0.3.0", "linux-x64", "validated"))

        check = verify.marketplace_round(
            "browsitory", "browsitory", "0.3.0", query=query, get_status=lambda url: 200
        )
        check("linux-x64")
        check("win32-x64")
        self.assertEqual(calls, [("browsitory", "browsitory")])

    def test_a_failed_query_reports_every_target_missing(self):
        def query(publisher, name):
            raise urllib.error.URLError("timed out")

        check = verify.marketplace_round("browsitory", "browsitory", "0.3.0", query=query)
        self.assertIn("extensionquery failed", check("linux-x64"))


class OpenVsxTests(unittest.TestCase):
    def answer(self, **overrides):
        data = {
            "version": "0.3.0",
            "targetPlatform": "linux-x64",
            "files": {"download": "https://open-vsx.org/x.vsix"},
        }
        data.update(overrides)
        return data

    def test_listed_with_matching_target_version_and_download(self):
        check = verify.open_vsx_round(
            "browsitory", "browsitory", "0.3.0", get_json=lambda url: (200, self.answer())
        )
        self.assertIsNone(check("linux-x64"))

    def test_url_names_target_and_version(self):
        urls = []

        def get_json(url):
            urls.append(url)
            return 404, None

        check = verify.open_vsx_round("browsitory", "browsitory", "0.3.0", get_json=get_json)
        self.assertEqual(check("linux-x64"), "answered 404")
        self.assertEqual(urls, ["https://open-vsx.org/api/browsitory/browsitory/linux-x64/0.3.0"])

    def test_wrong_target_or_no_download_is_missing(self):
        for data in (
            self.answer(targetPlatform="universal"),
            self.answer(version="0.2.0"),
            self.answer(files={}),
            self.answer(error="Extension not found"),
        ):
            with self.subTest(data=data):
                check = verify.open_vsx_round(
                    "browsitory", "browsitory", "0.3.0", get_json=lambda url, d=data: (200, d)
                )
                self.assertEqual(check("linux-x64"), "no download for this target")


class WaitForTargetsTests(unittest.TestCase):
    def test_returns_when_all_targets_appear(self):
        fake = FakeClock()
        rounds = iter(
            [
                {"darwin-arm64"},
                {"darwin-arm64", "linux-x64"},
            ]
        )

        def new_round():
            listed = next(rounds)
            return lambda target: None if target in listed else "missing"

        missing = verify.wait_for_targets(
            ["darwin-arm64", "linux-x64"],
            new_round,
            timeout=600,
            interval=30,
            log=lambda line: None,
            clock=fake.clock,
            sleep=fake.sleep,
        )
        self.assertEqual(missing, {})
        self.assertEqual(fake.sleeps, [30])

    def test_listed_targets_are_not_checked_again(self):
        fake = FakeClock()
        checked = []

        def new_round():
            def check(target):
                checked.append(target)
                return None if target == "darwin-arm64" else "missing"

            return check

        verify.wait_for_targets(
            ["darwin-arm64", "linux-x64"],
            new_round,
            timeout=60,
            interval=30,
            log=lambda line: None,
            clock=fake.clock,
            sleep=fake.sleep,
        )
        self.assertEqual(checked.count("darwin-arm64"), 1)
        self.assertEqual(checked.count("linux-x64"), 3)

    def test_times_out_with_the_last_reason_per_target(self):
        fake = FakeClock()
        missing = verify.wait_for_targets(
            ["linux-x64", "win32-x64"],
            lambda: (lambda target: f"{target} answered 404"),
            timeout=100,
            interval=30,
            log=lambda line: None,
            clock=fake.clock,
            sleep=fake.sleep,
        )
        self.assertEqual(
            missing,
            {"linux-x64": "linux-x64 answered 404", "win32-x64": "win32-x64 answered 404"},
        )
        self.assertEqual(fake.sleeps, [30, 30, 30, 10], "never sleeps past the deadline")

    def test_zero_timeout_still_checks_once(self):
        fake = FakeClock()
        rounds = []

        def new_round():
            rounds.append(1)
            return lambda target: "missing"

        verify.wait_for_targets(
            ["linux-x64"],
            new_round,
            timeout=0,
            interval=30,
            log=lambda line: None,
            clock=fake.clock,
            sleep=fake.sleep,
        )
        self.assertEqual(len(rounds), 1)
        self.assertEqual(fake.sleeps, [])


class PackagesDirTests(unittest.TestCase):
    def test_targets_come_from_the_package_names(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = pathlib.Path(tmp)
            for target in ("win32-x64", "darwin-arm64", "linux-x64", "darwin-x64"):
                (folder / f"browsitory-0.3.0-{target}.vsix").touch()
            self.assertEqual(
                verify.targets_from_packages(folder, "browsitory", "0.3.0"),
                ["darwin-arm64", "darwin-x64", "linux-x64", "win32-x64"],
            )

    def test_a_package_of_another_version_is_an_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = pathlib.Path(tmp)
            (folder / "browsitory-0.2.0-linux-x64.vsix").touch()
            with self.assertRaises(ValueError):
                verify.targets_from_packages(folder, "browsitory", "0.3.0")

    def test_an_empty_folder_is_an_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                verify.targets_from_packages(pathlib.Path(tmp), "browsitory", "0.3.0")


class MainTests(unittest.TestCase):
    def run_main(self, argv, listed):
        """Run main() with a fake Marketplace that lists the given targets for 0.3.0."""
        out = io.StringIO()
        with (
            mock.patch.object(
                verify,
                "query_marketplace",
                lambda p, n: query_result(*[("0.3.0", t, "validated") for t in listed]),
            ),
            mock.patch.object(verify, "http_get_status", lambda url: 200),
            mock.patch.object(verify.time, "sleep", lambda seconds: None),
            contextlib.redirect_stdout(out),
        ):
            code = verify.main(argv)
        return code, out.getvalue()

    def test_all_listed_passes(self):
        code, out = self.run_main(
            ["--version", "0.3.0", "darwin-arm64", "linux-x64"], ["darwin-arm64", "linux-x64"]
        )
        self.assertEqual(code, 0)
        self.assertIn("All 2 targets of 0.3.0 are listed", out)

    def test_missing_target_fails_with_rerun_advice(self):
        code, out = self.run_main(
            ["--version", "0.3.0", "--timeout", "0", "darwin-arm64", "linux-x64"],
            ["darwin-arm64"],
        )
        self.assertEqual(code, 1)
        self.assertIn("::error::", out)
        self.assertIn("for linux-x64 (", out)
        self.assertNotIn("darwin-arm64:", out)
        self.assertIn("Re-run this job; --skip-duplicate re-uploads only the missing targets", out)

    def test_bad_arguments_exit_2(self):
        for argv in (
            ["--version", "0.3.0"],
            ["--version", "0.3", "linux-x64"],
            ["--version", "0.3.0", "linux_x64"],
        ):
            with self.subTest(argv=argv):
                with contextlib.redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit) as raised:
                        verify.main(argv)
                self.assertEqual(raised.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
