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


class NormalizeTests(unittest.TestCase):
    def test_missing_or_see_file_is_unknown(self):
        for value in (None, "", "SEE LICENSE IN LICENSE.txt", "Unknown"):
            self.assertEqual(cdl.normalize_license(value), cdl.UNKNOWN)

    def test_legacy_object_and_list_forms(self):
        self.assertEqual(cdl.normalize_license({"type": "MIT"}), "MIT")
        self.assertEqual(cdl.normalize_license([{"type": "MIT"}, {"type": "Apache-2.0"}]), "MIT OR Apache-2.0")


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


if __name__ == "__main__":
    unittest.main()
