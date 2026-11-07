#!/usr/bin/env python3
"""Tests for check-dependency-licenses.py (no network, no pnpm)."""

import importlib.util
import pathlib
import sys
import tempfile
import unittest

MODULE_PATH = pathlib.Path(__file__).resolve().parent / "check-dependency-licenses.py"
spec = importlib.util.spec_from_file_location("check_dependency_licenses", MODULE_PATH)
cdl = importlib.util.module_from_spec(spec)
assert spec.loader is not None
sys.modules[spec.name] = cdl  # dataclasses look their module up while the class is built
spec.loader.exec_module(cdl)

ALLOWED = cdl.ALLOWED


def pkg(name, license, version="1.0.0", production=False, project="frontend"):
    return cdl.Package(project, name, version, license, production)


def check_alone(*packages):
    """Problems for these packages, ignoring unused-table-entry reports."""
    return cdl.check_packages(list(packages))[0]


class SatisfiesTests(unittest.TestCase):
    def test_single_allowed(self):
        self.assertTrue(cdl.satisfies("MIT", ALLOWED))

    def test_single_disallowed(self):
        self.assertFalse(cdl.satisfies("MPL-2.0", ALLOWED))

    def test_or_needs_one_allowed_side(self):
        self.assertTrue(cdl.satisfies("(MIT OR GPL-3.0-or-later)", ALLOWED))
        self.assertFalse(cdl.satisfies("GPL-3.0-only OR MPL-2.0", ALLOWED))

    def test_and_needs_every_side(self):
        self.assertTrue(cdl.satisfies("(MIT AND Zlib)", ALLOWED))
        self.assertFalse(cdl.satisfies("MIT AND CC-BY-4.0", ALLOWED))

    def test_and_binds_tighter_than_or(self):
        self.assertTrue(cdl.satisfies("MPL-2.0 AND CC0-1.0 OR MIT", ALLOWED))
        self.assertFalse(cdl.satisfies("MIT AND (MPL-2.0 OR CC0-1.0)", ALLOWED))

    def test_with_needs_the_whole_term(self):
        self.assertFalse(cdl.satisfies("Apache-2.0 WITH LLVM-exception", ALLOWED))
        self.assertTrue(cdl.satisfies("Apache-2.0 WITH LLVM-exception", ALLOWED | {"Apache-2.0 WITH LLVM-exception"}))

    def test_the_newly_allowed_licences(self):
        for lic in ("BlueOak-1.0.0", "PSF-2.0", "Python-2.0"):
            self.assertTrue(cdl.satisfies(lic, ALLOWED), lic)

    def test_malformed_is_never_satisfied(self):
        for expr in ("", "MIT OR", "(MIT", "MIT)", "OR MIT", "MIT MIT"):
            self.assertFalse(cdl.satisfies(expr, ALLOWED), expr)

    def test_lowercase_ids_and_operators_fail_closed(self):
        for expr in ("mit", "MIT or Apache-2.0", "MIT and Zlib", "GPL-3.0-only or MIT"):
            self.assertFalse(cdl.satisfies(expr, ALLOWED), expr)


class NormalizeTests(unittest.TestCase):
    def test_missing_or_see_file_is_unknown(self):
        for value in (None, "", "SEE LICENSE IN LICENSE.txt", "Unknown"):
            self.assertEqual(cdl.normalize_license(value), cdl.UNKNOWN)

    def test_legacy_object_form(self):
        self.assertEqual(cdl.normalize_license({"type": "MIT"}), "MIT")

    def test_legacy_list_form_requires_every_licence(self):
        # A `licenses` array doesn't say the user may choose, so every entry must be allowed.
        self.assertEqual(cdl.normalize_license([{"type": "MIT"}, {"type": "Apache-2.0"}]), "MIT AND Apache-2.0")
        self.assertFalse(cdl.satisfies(cdl.normalize_license([{"type": "MIT"}, {"type": "GPL-2.0-only"}]), ALLOWED))
        self.assertTrue(cdl.satisfies(cdl.normalize_license([{"type": "MIT"}, "Zlib"]), ALLOWED))


class MatchingTests(unittest.TestCase):
    def test_version_prefix_matches_whole_components(self):
        self.assertTrue(cdl.version_matches("1", "1.33.0"))
        self.assertFalse(cdl.version_matches("1", "10.0.0"))
        self.assertFalse(cdl.version_matches("1.3", "1.33.0"))
        self.assertTrue(cdl.version_matches("0.0.1", "0.0.1"))
        self.assertFalse(cdl.version_matches("0.0.1", "0.0.10"))

    def test_platform_globs_dont_overreach(self):
        self.assertEqual(check_alone(pkg("@vscode/vsce-sign", cdl.UNKNOWN, "2.1.0")), [])
        self.assertEqual(check_alone(pkg("@vscode/vsce-sign-linux-arm", cdl.UNKNOWN, "2.0.6")), [])
        for name in ("@vscode/vsce-signature-x", "@vscode/vsce-sign-x", "@vscode/vsce-signer"):
            self.assertEqual(len(check_alone(pkg(name, cdl.UNKNOWN, "2.0.0"))), 1, name)
        self.assertEqual(check_alone(pkg("lightningcss-linux-x64-gnu", "MPL-2.0", "1.33.0")), [])
        for name in ("lightningcss-plugin", "lightningcss2", "lightningcssx-a-b"):
            self.assertEqual(len(check_alone(pkg(name, "MPL-2.0", "1.33.0"))), 1, name)


class CheckPackagesTests(unittest.TestCase):
    def test_allowed_licences_pass(self):
        self.assertEqual(check_alone(pkg("react", "MIT", production=True), pkg("glob", "BlueOak-1.0.0")), [])

    def test_disallowed_dev_package_without_exception_fails(self):
        problems = check_alone(pkg("some-gpl-tool", "GPL-3.0-only"))
        self.assertEqual(len(problems), 1)
        self.assertIn("some-gpl-tool@1.0.0 (GPL-3.0-only)", problems[0])

    def test_exception_covers_its_dev_package(self):
        self.assertEqual(check_alone(pkg("lightningcss", "MPL-2.0", "1.33.0")), [])
        self.assertEqual(check_alone(pkg("lightningcss-darwin-arm64", "MPL-2.0", "1.33.0")), [])

    def test_exception_is_scoped_to_its_version_range(self):
        self.assertEqual(len(check_alone(pkg("lightningcss", "MPL-2.0", "2.0.0"))), 1)

    def test_exception_is_scoped_to_its_licence(self):
        self.assertEqual(len(check_alone(pkg("lightningcss", "GPL-3.0-only", "1.33.0"))), 1)

    def test_exception_never_covers_a_production_package(self):
        problems = check_alone(pkg("lightningcss", "MPL-2.0", "1.33.0", production=True))
        self.assertEqual(len(problems), 1)
        self.assertIn("production tree", problems[0])

    def test_unknown_licence_fails_unless_excepted(self):
        self.assertEqual(len(check_alone(pkg("mystery", cdl.UNKNOWN))), 1)
        self.assertEqual(check_alone(pkg("@vscode/vsce-sign-win32-x64", cdl.UNKNOWN, "2.0.6")), [])

    def test_clarification_replaces_the_declared_licence(self):
        self.assertEqual(check_alone(pkg("css-value", cdl.UNKNOWN, "0.0.1", project="e2e")), [])
        self.assertEqual(len(check_alone(pkg("css-value", cdl.UNKNOWN, "0.0.2", project="e2e"))), 1)

    def test_unused_table_entries_are_reported(self):
        _, unused = cdl.check_packages([pkg("react", "MIT")])
        self.assertIn("clarification css-value 0.0.1", unused)
        self.assertTrue(any(entry.startswith("exception lightningcss ") for entry in unused))

    def test_no_unused_entries_when_every_entry_matches(self):
        packages = [pkg(e.name.replace("*", "x"), e.license, e.versions + ".0.0") for e in cdl.EXCEPTIONS]
        packages += [pkg(c.name, cdl.UNKNOWN, c.versions) for c in cdl.CLARIFICATIONS]
        problems, unused = cdl.check_packages(packages)
        self.assertEqual((problems, unused), ([], []))


LOCKFILE = """\
lockfileVersion: '9.0'

importers:

  .:
    dependencies:
      vite:
        specifier: ^8
        version: 8.3.5

packages:

  '@scope/thing@2.0.0':
    resolution: {integrity: sha512-x}

  lightningcss-darwin-arm64@1.33.0:
    resolution: {integrity: sha512-y}
    cpu: [arm64]
    os: [darwin]

  lightningcss@1.33.0:
    resolution: {integrity: sha512-z}

  string-width@4.2.3:
    resolution: {integrity: sha512-w}

snapshots:

  '@scope/thing@2.0.0(peer@1.0.0)':
    dependencies:
      string-width-cjs: string-width@4.2.3

  lightningcss-darwin-arm64@1.33.0:
    optional: true

  lightningcss@1.33.0:
    dependencies:
      '@scope/thing': 2.0.0(peer@1.0.0)
    optionalDependencies:
      lightningcss-darwin-arm64: 1.33.0

  string-width@4.2.3: {}
"""

ENV_DOCUMENT = """\
---
lockfileVersion: '9.0'

importers:

  .:
    configDependencies: {}
    packageManagerDependencies:
      pnpm:
        specifier: 12.18.0
        version: 12.18.0

packages:

  '@pnpm/exe.linux-x64@12.18.0':
    resolution: {integrity: sha512-a}
    cpu: [x64]
    os: [linux]

  pnpm@12.18.0:
    resolution: {integrity: sha512-b}

snapshots:

  '@pnpm/exe.linux-x64@12.18.0':
    optional: true

  pnpm@12.18.0:
    optionalDependencies:
      '@pnpm/exe.linux-x64': 12.18.0
"""


class LockfileTests(unittest.TestCase):
    def test_packages_and_children(self):
        packages, children = cdl.parse_lockfile(LOCKFILE)
        self.assertEqual(
            packages,
            {
                ("@scope/thing", "2.0.0"),
                ("lightningcss-darwin-arm64", "1.33.0"),
                ("lightningcss", "1.33.0"),
                ("string-width", "4.2.3"),
            },
        )
        self.assertEqual(
            children[("lightningcss", "1.33.0")],
            {("@scope/thing", "2.0.0"), ("lightningcss-darwin-arm64", "1.33.0")},
        )
        self.assertEqual(children[("@scope/thing", "2.0.0")], {("string-width", "4.2.3")})

    def test_env_lockfile_document_is_skipped(self):
        # pnpm 11+ writes the env lockfile (the pinned pnpm itself) as a first YAML document.
        with_env = ENV_DOCUMENT + "\n---\n" + LOCKFILE
        self.assertEqual(cdl.parse_lockfile(with_env), cdl.parse_lockfile(LOCKFILE))

    def test_unexpected_documents_fail(self):
        # Two project documents, or a first document that isn't an env lockfile.
        for text in (LOCKFILE + "\n---\n" + LOCKFILE, ENV_DOCUMENT + "\n---\n" + ENV_DOCUMENT + "\n---\n" + LOCKFILE):
            with self.assertRaises(cdl.CheckError):
                cdl.parse_lockfile(text)

    def test_unknown_lockfile_version_fails(self):
        for first_line in ("lockfileVersion: '10.0'", "lockfileVersion: 6.0", "packages:"):
            with self.assertRaises(cdl.CheckError, msg=first_line):
                cdl.parse_lockfile(LOCKFILE.replace("lockfileVersion: '9.0'", first_line, 1))

    def test_unexpected_layout_fails(self):
        # Same version, but the entries are indented differently: nothing would parse.
        reindented = LOCKFILE.replace("\n  ", "\n   ")
        with self.assertRaises(cdl.CheckError):
            cdl.parse_lockfile(reindented)
        # A `packages:` entry with no snapshot.
        with self.assertRaises(cdl.CheckError):
            cdl.parse_lockfile(LOCKFILE.replace("  string-width@4.2.3: {}\n", ""))

    def test_parse_licenses_json(self):
        data = {"MIT": [{"name": "a", "versions": ["1.0.0", "2.0.0"], "license": "MIT"}], "Unknown": [{"name": "b", "versions": ["1.0.0"]}]}
        self.assertEqual(
            cdl.parse_licenses_json(data),
            {("a", "1.0.0"): "MIT", ("a", "2.0.0"): "MIT", ("b", "1.0.0"): cdl.UNKNOWN},
        )


class CollectProjectTests(unittest.TestCase):
    """Other-platform binaries come from the lookup; production status follows the parent."""

    def setUp(self):
        self.orig_root = cdl.ROOT
        self.dir = tempfile.TemporaryDirectory()
        cdl.ROOT = pathlib.Path(self.dir.name)
        (cdl.ROOT / "proj").mkdir()
        (cdl.ROOT / "proj" / "pnpm-lock.yaml").write_text(LOCKFILE)

    def tearDown(self):
        cdl.ROOT = self.orig_root
        self.dir.cleanup()

    def run_pnpm(self, prod_includes_lightningcss):
        def run(project, prod):
            self.assertEqual(project, "proj")
            entries = [
                {"name": "lightningcss", "versions": ["1.33.0"], "license": "MPL-2.0"},
                {"name": "@scope/thing", "versions": ["2.0.0"], "license": "MIT"},
                {"name": "string-width", "versions": ["4.2.3"], "license": "MIT"},
            ]
            if prod and not prod_includes_lightningcss:
                return {}
            return {"x": entries}

        return run

    def test_missing_lockfile_entries_are_looked_up(self):
        looked_up = []

        def lookup(name, version):
            looked_up.append((name, version))
            return "MPL-2.0"

        packages = cdl.collect_project("proj", False, lookup=lookup, run=self.run_pnpm(False))
        self.assertEqual(looked_up, [("lightningcss-darwin-arm64", "1.33.0")])
        by_name = {p.name: p for p in packages}
        self.assertEqual(by_name["lightningcss-darwin-arm64"].license, "MPL-2.0")
        self.assertFalse(by_name["lightningcss-darwin-arm64"].production)

    def test_skipped_binary_of_a_production_package_is_production(self):
        packages = cdl.collect_project("proj", True, lookup=lambda n, v: "MPL-2.0", run=self.run_pnpm(True))
        by_name = {p.name: p for p in packages}
        self.assertTrue(by_name["lightningcss-darwin-arm64"].production)
        problems = check_alone(*packages)
        self.assertTrue(any("lightningcss-darwin-arm64" in p and "production tree" in p for p in problems))

    def test_installed_package_missing_from_lockfile_fails(self):
        def run(project, prod):
            return {"MIT": [{"name": "not-in-lockfile", "versions": ["1.0.0"], "license": "MIT"}]}

        with self.assertRaises(cdl.CheckError):
            cdl.collect_project("proj", False, lookup=lambda n, v: "MIT", run=run)

    def test_production_status_follows_several_skipped_hops(self):
        lockfile = """\
lockfileVersion: '9.0'

packages:

  app-dep@1.0.0:
    resolution: {integrity: sha512-a}

  native@1.0.0:
    resolution: {integrity: sha512-b}

  native-sub@1.0.0:
    resolution: {integrity: sha512-c}

  dev-only@1.0.0:
    resolution: {integrity: sha512-d}

snapshots:

  app-dep@1.0.0:
    optionalDependencies:
      native: 1.0.0

  native@1.0.0:
    dependencies:
      native-sub: 1.0.0

  native-sub@1.0.0: {}

  dev-only@1.0.0: {}
"""
        (cdl.ROOT / "proj" / "pnpm-lock.yaml").write_text(lockfile)

        def run(project, prod):
            entries = [{"name": "app-dep", "versions": ["1.0.0"], "license": "MIT"}]
            if not prod:
                entries.append({"name": "dev-only", "versions": ["1.0.0"], "license": "MIT"})
            return {"MIT": entries}

        packages = cdl.collect_project("proj", True, lookup=lambda n, v: "MIT", run=run)
        production = {p.name for p in packages if p.production}
        self.assertEqual(production, {"app-dep", "native", "native-sub"})


class PnpmLicensesTests(unittest.TestCase):
    def test_runs_the_resolved_pnpm_and_decodes_utf8(self):
        calls = []

        class Done:
            stdout = '{"MIT": [{"name": "a", "versions": ["1.0.0"], "author": "Zoë"}]}'

        def run(cmd, **kwargs):
            calls.append((cmd, kwargs))
            return Done()

        data = cdl.pnpm_licenses("extension/e2e", True, run=run, which=lambda name: r"C:\pnpm\pnpm.cmd")
        self.assertEqual(data["MIT"][0]["author"], "Zoë")
        cmd, kwargs = calls[0]
        self.assertEqual(cmd[0], r"C:\pnpm\pnpm.cmd")
        self.assertIn("--prod", cmd)
        self.assertIn("--ignore-workspace", cmd)
        self.assertEqual(kwargs["encoding"], "utf-8")

    def test_missing_pnpm_is_a_check_error(self):
        with self.assertRaisesRegex(cdl.CheckError, "pnpm not found"):
            cdl.pnpm_licenses("frontend", False, which=lambda name: None)


class FakeResponse:
    def __init__(self, body):
        self.body = body

    def __enter__(self):
        import io

        return io.BytesIO(self.body)

    def __exit__(self, *exc):
        return False


class RegistryTests(unittest.TestCase):
    def fake_urlopen(self, *results):
        calls = []

        def urlopen(url, timeout):
            calls.append(url)
            result = results[min(len(calls), len(results)) - 1]
            if isinstance(result, Exception):
                raise result
            return FakeResponse(result)

        return urlopen, calls

    def http_error(self, code):
        import urllib.error

        return urllib.error.HTTPError("https://registry.npmjs.org/x", code, "err", {}, None)

    def test_reads_the_licence(self):
        urlopen, calls = self.fake_urlopen(b'{"name": "@scope/x", "license": "MIT"}')
        self.assertEqual(cdl.registry_license("@scope/x", "1.0.0", urlopen=urlopen, sleep=lambda s: None), "MIT")
        self.assertEqual(calls, ["https://registry.npmjs.org/@scope%2Fx/1.0.0"])

    def test_404_fails_at_once_and_names_the_package(self):
        urlopen, calls = self.fake_urlopen(self.http_error(404))
        with self.assertRaisesRegex(cdl.CheckError, r"gone@1\.2\.3.*HTTP 404"):
            cdl.registry_license("gone", "1.2.3", urlopen=urlopen, sleep=lambda s: None)
        self.assertEqual(len(calls), 1)

    def test_server_errors_are_retried(self):
        urlopen, calls = self.fake_urlopen(self.http_error(503), b'{"license": "ISC"}')
        self.assertEqual(cdl.registry_license("x", "1.0.0", urlopen=urlopen, sleep=lambda s: None), "ISC")
        self.assertEqual(len(calls), 2)

    def test_malformed_json_fails_with_the_package_name(self):
        for body in (b"<html>", b"[1, 2]"):
            urlopen, _ = self.fake_urlopen(body)
            with self.assertRaisesRegex(cdl.CheckError, r"x@1\.0\.0.*bad JSON"):
                cdl.registry_license("x", "1.0.0", urlopen=urlopen, sleep=lambda s: None)


class MainTests(unittest.TestCase):
    def run_main(self, collect):
        import contextlib
        import io

        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = cdl.main([], collect=collect)
        return code, out.getvalue(), err.getvalue()

    def every_entry(self, project, ships):
        if project != "e2e":
            return []
        packages = [pkg(e.name.replace("*", "x"), e.license, e.versions + ".0.0", project=project) for e in cdl.EXCEPTIONS]
        packages += [pkg(c.name, cdl.UNKNOWN, c.versions, project=project) for c in cdl.CLARIFICATIONS]
        return packages

    def test_passes_when_every_package_is_allowed_or_excepted(self):
        code, out, err = self.run_main(self.every_entry)
        self.assertEqual((code, err), (0, ""))
        self.assertIn("Licences OK", out)

    def test_fails_on_a_disallowed_licence(self):
        def collect(project, ships):
            extra = [pkg("bad", "GPL-3.0-only", project=project)] if project == "frontend" else []
            return self.every_entry(project, ships) + extra

        code, _, err = self.run_main(collect)
        self.assertEqual(code, 1)
        self.assertIn("frontend: bad@1.0.0 (GPL-3.0-only)", err)

    def test_fails_on_an_unused_entry(self):
        code, _, err = self.run_main(lambda project, ships: [])
        self.assertEqual(code, 1)
        self.assertIn("clarification css-value 0.0.1 matches no package", err)

    def test_check_error_is_reported_not_raised(self):
        def collect(project, ships):
            raise cdl.CheckError("npm registry lookup of gone@1.2.3 failed: HTTP 404")

        code, _, err = self.run_main(collect)
        self.assertEqual(code, 1)
        self.assertIn("error: npm registry lookup of gone@1.2.3 failed: HTTP 404", err)


if __name__ == "__main__":
    unittest.main()
