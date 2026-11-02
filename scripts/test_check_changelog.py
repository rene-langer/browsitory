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

    def test_cargo_manifest_and_lock_only(self):
        self.assertFalse(
            check_changelog.needs_changelog(
                [
                    "Cargo.lock",
                    "crates/git-core/Cargo.toml",
                    "crates/tauri-app/Cargo.toml",
                ]
            )
        )

    def test_pnpm_manifest_and_lockfiles_only(self):
        self.assertFalse(
            check_changelog.needs_changelog(
                [
                    "e2e/package.json",
                    "e2e/pnpm-lock.yaml",
                    "frontend/package.json",
                    "frontend/pnpm-lock.yaml",
                    "extension/e2e/pnpm-workspace.yaml",
                ]
            )
        )

    def test_rust_toolchain_only(self):
        self.assertFalse(check_changelog.needs_changelog(["rust-toolchain.toml"]))

    def test_dependency_files_plus_watched_code_still_needs_changelog(self):
        self.assertTrue(
            check_changelog.needs_changelog(
                ["crates/git-core/Cargo.toml", "crates/git-core/src/lib.rs"]
            )
        )

    def test_lookalike_file_names_are_not_exempt(self):
        self.assertTrue(
            check_changelog.needs_changelog(["crates/git-core/tests/Cargo.toml.rs"])
        )
        self.assertTrue(
            check_changelog.needs_changelog(["e2e/specs/package.json.spec.ts"])
        )


if __name__ == "__main__":
    unittest.main()
