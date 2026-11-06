#!/usr/bin/env python3
"""Tests for verify-marketplace-listing.py. No network: the HTTP calls are replaced by fakes."""

import contextlib
import email.message
import http.client
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

        check = verify.MarketplaceRound(
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

        check = verify.MarketplaceRound(
            "browsitory", "browsitory", "0.3.0", query=query, get_status=lambda url: 200
        )
        check("linux-x64")
        check("win32-x64")
        self.assertEqual(calls, [("browsitory", "browsitory")])

    def test_a_failed_query_reports_every_target_missing(self):
        def query(publisher, name):
            raise urllib.error.URLError("timed out")

        check = verify.MarketplaceRound("browsitory", "browsitory", "0.3.0", query=query)
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
        check = verify.OpenVsxRound(
            "browsitory", "browsitory", "0.3.0", get_json=lambda url: (200, self.answer())
        )
        self.assertIsNone(check("linux-x64"))

    def test_url_names_target_and_version(self):
        urls = []

        def get_json(url):
            urls.append(url)
            return 404, None

        check = verify.OpenVsxRound("browsitory", "browsitory", "0.3.0", get_json=get_json)
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
                check = verify.OpenVsxRound(
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


def http_error(code, retry_after=None):
    headers = email.message.Message()
    if retry_after is not None:
        headers["Retry-After"] = retry_after
    return urllib.error.HTTPError("https://example.invalid", code, "error", headers, None)


class FakeResponse:
    def __init__(self, status=200, body=b"{}"):
        self.status = status
        self.body = io.BytesIO(body)

    def read(self, *args):
        return self.body.read(*args)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


def urlopen_raising(error):
    def urlopen(request, timeout=None):
        raise error

    return urlopen


def raising(error):
    def call(*args):
        raise error

    return call


def validated(*targets):
    return lambda publisher, name: query_result(*[("0.3.0", t, "validated") for t in targets])


OPEN_VSX_LINUX = {"version": "0.3.0", "targetPlatform": "linux-x64", "files": {"download": "x"}}


class HttpHelperTests(unittest.TestCase):
    """The real HTTP helpers with urllib.request.urlopen patched: no network."""

    def patched(self, urlopen):
        return mock.patch.object(verify.urllib.request, "urlopen", urlopen)

    def test_503_is_transient_on_every_path(self):
        with self.patched(urlopen_raising(http_error(503))):
            self.assertEqual(verify.http_get_status("https://x.invalid/a"), 503)
            self.assertEqual(verify.http_get_json("https://x.invalid/a"), (503, None))
            check = verify.MarketplaceRound("browsitory", "browsitory", "0.3.0")
            self.assertIn("extensionquery failed", check("linux-x64"))
            self.assertIn("503", check("linux-x64"))
            self.assertIsNone(check.retry_after)
            open_vsx = verify.OpenVsxRound("browsitory", "browsitory", "0.3.0")
            self.assertEqual(open_vsx("linux-x64"), "answered 503")

    def test_429_raises_throttled_with_the_capped_retry_after(self):
        for value, expected in (("120", 120), ("100000", 300), ("soon", 60), (None, 60)):
            calls = (
                lambda: verify.http_get_status("https://x.invalid/a"),
                lambda: verify.http_get_json("https://x.invalid/a"),
                lambda: verify.query_marketplace("browsitory", "browsitory"),
            )
            for call in calls:
                with self.subTest(retry_after=value), self.patched(
                    urlopen_raising(http_error(429, value))
                ):
                    with self.assertRaises(verify.Throttled) as raised:
                        call()
                    self.assertEqual(raised.exception.retry_after, expected)

    def test_429_in_a_round_is_a_reason_with_a_retry_after(self):
        with self.patched(urlopen_raising(http_error(429, "90"))):
            marketplace = verify.MarketplaceRound("browsitory", "browsitory", "0.3.0")
            self.assertIn("429", marketplace("linux-x64"))
            self.assertEqual(marketplace.retry_after, 90)
            open_vsx = verify.OpenVsxRound("browsitory", "browsitory", "0.3.0")
            self.assertIn("429", open_vsx("linux-x64"))
            self.assertEqual(open_vsx.retry_after, 90)

    def test_429_on_the_download_check_is_a_reason(self):
        check = verify.MarketplaceRound(
            "browsitory",
            "browsitory",
            "0.3.0",
            query=validated("linux-x64"),
            get_status=raising(verify.Throttled(45)),
        )
        self.assertIn("429", check("linux-x64"))
        self.assertEqual(check.retry_after, 45)

    def test_broken_responses_are_transient_on_every_path(self):
        with self.patched(urlopen_raising(http.client.IncompleteRead(b"partial", 100))):
            check = verify.MarketplaceRound("browsitory", "browsitory", "0.3.0")
            self.assertIn("extensionquery failed", check("linux-x64"))
            open_vsx = verify.OpenVsxRound("browsitory", "browsitory", "0.3.0")
            self.assertIn("request failed", open_vsx("linux-x64"))
        check = verify.MarketplaceRound(
            "browsitory",
            "browsitory",
            "0.3.0",
            query=validated("linux-x64"),
            get_status=raising(http.client.BadStatusLine("garbage")),
        )
        self.assertIn("download check failed", check("linux-x64"))

    def test_a_body_that_is_not_json_is_transient(self):
        with self.patched(lambda request, timeout=None: FakeResponse(200, b"<html>")):
            check = verify.MarketplaceRound("browsitory", "browsitory", "0.3.0")
            self.assertIn("extensionquery failed", check("linux-x64"))
            open_vsx = verify.OpenVsxRound("browsitory", "browsitory", "0.3.0")
            self.assertIn("request failed", open_vsx("linux-x64"))

    def test_open_vsx_network_error_is_transient(self):
        check = verify.OpenVsxRound(
            "browsitory",
            "browsitory",
            "0.3.0",
            get_json=raising(urllib.error.URLError("connection reset")),
        )
        self.assertIn("request failed", check("linux-x64"))


class ThrottleBackoffTests(unittest.TestCase):
    def test_waits_the_retry_after_when_longer_than_the_interval(self):
        fake = FakeClock()
        answers = iter([raising(verify.Throttled(120)), lambda url: (200, OPEN_VSX_LINUX)])

        def new_round():
            return verify.OpenVsxRound("browsitory", "browsitory", "0.3.0", get_json=next(answers))

        missing = verify.wait_for_targets(
            ["linux-x64"],
            new_round,
            timeout=600,
            interval=30,
            log=lambda line: None,
            clock=fake.clock,
            sleep=fake.sleep,
        )
        self.assertEqual(missing, {})
        self.assertEqual(fake.sleeps, [120])

    def test_never_sleeps_past_the_deadline_when_throttled(self):
        fake = FakeClock()
        missing = verify.wait_for_targets(
            ["linux-x64"],
            lambda: verify.OpenVsxRound(
                "browsitory", "browsitory", "0.3.0", get_json=raising(verify.Throttled(300))
            ),
            timeout=100,
            interval=30,
            log=lambda line: None,
            clock=fake.clock,
            sleep=fake.sleep,
        )
        self.assertIn("429", missing["linux-x64"])
        self.assertEqual(fake.sleeps, [100])

    def test_uses_time_sleep_and_monotonic_at_call_time(self):
        slept = []
        with (
            mock.patch.object(verify.time, "sleep", slept.append),
            mock.patch.object(verify.time, "monotonic", iter([0, 0, 100]).__next__),
        ):
            verify.wait_for_targets(
                ["linux-x64"],
                lambda: (lambda target: "missing"),
                timeout=50,
                interval=30,
                log=lambda line: None,
            )
        self.assertEqual(slept, [30])


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

    def test_waits_with_the_interval_until_listed(self):
        rounds = iter([validated(), validated("linux-x64")])
        slept = []
        with (
            mock.patch.object(verify, "query_marketplace", lambda p, n: next(rounds)(p, n)),
            mock.patch.object(verify, "http_get_status", lambda url: 200),
            mock.patch.object(verify.time, "sleep", slept.append),
            contextlib.redirect_stdout(io.StringIO()),
        ):
            code = verify.main(["--version", "0.3.0", "--interval", "7", "linux-x64"])
        self.assertEqual(code, 0)
        self.assertEqual(slept, [7])

    def test_open_vsx_registry(self):
        def get_json(url):
            target = url.split("/")[-2]
            if target == "win32-x64":
                return 404, None
            return 200, {"version": "0.3.0", "targetPlatform": target, "files": {"download": "x"}}

        out = io.StringIO()
        argv = ["--registry", "open-vsx", "--version", "0.3.0", "--timeout", "0"]
        with (
            mock.patch.object(verify, "http_get_json", get_json),
            contextlib.redirect_stdout(out),
        ):
            code = verify.main(argv + ["linux-x64", "win32-x64"])
        self.assertEqual(code, 1)
        self.assertIn("listed: linux-x64", out.getvalue())
        self.assertIn(
            "::error::Missing on Open VSX after 0 s: 0.3.0 for win32-x64 (", out.getvalue()
        )

    def test_bad_arguments_exit_2(self):
        for extra in (
            [],
            ["--interval", "0", "linux-x64"],
            ["--interval", "-5", "linux-x64"],
            ["--interval", "nan", "linux-x64"],
            ["--timeout", "-1", "linux-x64"],
            ["--publisher", "Bad Publisher", "linux-x64"],
            ["--name", "x/y", "linux-x64"],
            ["linux_x64"],
        ):
            argv = ["--version", "0.3.0"] + extra
            with self.subTest(argv=argv):
                with contextlib.redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit) as raised:
                        verify.main(argv)
                self.assertEqual(raised.exception.code, 2)

    def test_bad_version_exits_2(self):
        for argv in (["--version", "0.3", "linux-x64"],):
            with self.subTest(argv=argv):
                with contextlib.redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit) as raised:
                        verify.main(argv)
                self.assertEqual(raised.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
