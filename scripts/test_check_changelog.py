#!/usr/bin/env python3
"""Tests for check-changelog.py's pure changelog-required predicate."""

import importlib.util
import pathlib
import unittest

MODULE_PATH = pathlib.Path(__file__).resolve().parent / "check-changelog.py"
spec = importlib.util.spec_from_file_location("check_changelog", MODULE_PATH)
check_changelog = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(check_changelog)


class NeedsChangelogTests(unittest.TestCase):
    def test_no_watched_files_changed(self):
        self.assertFalse(check_changelog.needs_changelog(["docs/README.md"]))

    def test_watched_crate_file_without_changelog(self):
        self.assertTrue(
            check_changelog.needs_changelog(["crates/git-core/src/lib.rs"])
        )

    def test_watched_frontend_file_without_changelog(self):
        self.assertTrue(check_changelog.needs_changelog(["frontend/src/App.tsx"]))

    def test_watched_e2e_file_without_changelog(self):
        self.assertTrue(
            check_changelog.needs_changelog(["e2e/specs/pull-requests.spec.ts"])
        )

    def test_watched_file_with_changelog_update(self):
        self.assertFalse(
            check_changelog.needs_changelog(
                ["crates/git-core/src/lib.rs", "CHANGELOG.md"]
            )
        )

    def test_no_files_changed(self):
        self.assertFalse(check_changelog.needs_changelog([]))

    def test_only_changelog_changed(self):
        self.assertFalse(check_changelog.needs_changelog(["CHANGELOG.md"]))


if __name__ == "__main__":
    unittest.main()
