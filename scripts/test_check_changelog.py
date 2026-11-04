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


class FragmentTests(unittest.TestCase):
    def test_fragment_counts_as_changelog_update(self):
        self.assertFalse(
            check_changelog.needs_changelog(
                ["crates/git-core/src/lib.rs", "changelog.d/240-leftover-state.md"]
            )
        )

    def test_fragment_without_issue_number_counts(self):
        self.assertFalse(
            check_changelog.needs_changelog(
                ["frontend/src/App.tsx", "changelog.d/changelog-fragments.md"]
            )
        )

    def test_readme_in_fragment_dir_does_not_count(self):
        self.assertTrue(
            check_changelog.needs_changelog(
                ["crates/git-core/src/lib.rs", "changelog.d/README.md"]
            )
        )

    def test_non_markdown_file_in_fragment_dir_does_not_count(self):
        self.assertTrue(
            check_changelog.needs_changelog(
                ["crates/git-core/src/lib.rs", "changelog.d/240-leftover.txt"]
            )
        )

    def test_markdown_in_a_subfolder_does_not_count(self):
        self.assertTrue(
            check_changelog.needs_changelog(
                ["crates/git-core/src/lib.rs", "changelog.d/old/240-leftover.md"]
            )
        )

    def test_fragment_files_are_selected_from_changed_files(self):
        self.assertEqual(
            check_changelog.fragment_files(
                [
                    "changelog.d/README.md",
                    "changelog.d/240-a.md",
                    "crates/git-core/src/lib.rs",
                    "CHANGELOG.md",
                ]
            ),
            ["changelog.d/240-a.md"],
        )


class ParseNameStatusTests(unittest.TestCase):
    def test_deleted_fragments_are_dropped_but_other_deletions_kept(self):
        output = (
            "M\tCHANGELOG.md\n"
            "D\tchangelog.d/240-a.md\n"
            "D\tcrates/git-core/src/old.rs\n"
            "A\tchangelog.d/241-b.md\n"
        )
        self.assertEqual(
            check_changelog.parse_name_status(output),
            ["CHANGELOG.md", "crates/git-core/src/old.rs", "changelog.d/241-b.md"],
        )

    def test_deleting_a_fragment_does_not_count_as_an_entry(self):
        files = check_changelog.parse_name_status(
            "M\tcrates/git-core/src/lib.rs\nD\tchangelog.d/240-a.md\n"
        )
        self.assertTrue(check_changelog.needs_changelog(files))


class FragmentProblemTests(unittest.TestCase):
    def test_valid_fragment(self):
        text = "### Leftover state\n\nIntro.\n\n- Fixed: amend works (#240).\n"
        self.assertEqual(check_changelog.fragment_problems(text), [])

    def test_leading_blank_lines_are_fine(self):
        self.assertEqual(
            check_changelog.fragment_problems("\n\n### Topic\n\n- Fixed: x.\n"), []
        )

    def test_empty_fragment(self):
        self.assertTrue(check_changelog.fragment_problems(""))
        self.assertTrue(check_changelog.fragment_problems("  \n\n"))

    def test_fragment_must_start_with_a_level_three_heading(self):
        self.assertTrue(check_changelog.fragment_problems("- Fixed: x (#1).\n"))
        self.assertTrue(check_changelog.fragment_problems("## Topic\n\n- Fixed: x.\n"))
        self.assertTrue(check_changelog.fragment_problems("###Topic\n"))

    def test_heading_needs_a_title(self):
        self.assertTrue(check_changelog.fragment_problems("### \n\n- Fixed: x.\n"))

    def test_heading_alone_is_not_enough(self):
        self.assertTrue(check_changelog.fragment_problems("### Topic\n"))

    def test_fragment_must_not_contain_top_level_headers(self):
        self.assertTrue(
            check_changelog.fragment_problems(
                "### Topic\n\n- Fixed: x.\n\n## [Unreleased]\n"
            )
        )
        self.assertTrue(
            check_changelog.fragment_problems("### Topic\n\n# Changelog\n- x\n")
        )

    def test_hash_lines_inside_code_fences_are_fine(self):
        self.assertEqual(
            check_changelog.fragment_problems(
                "### Topic\n\n- Run:\n\n  ```bash\n  # build\n  ```\n"
            ),
            [],
        )

    def test_level_four_headings_inside_are_fine(self):
        self.assertEqual(
            check_changelog.fragment_problems("### Topic\n\n#### Detail\n\n- x\n"),
            [],
        )


if __name__ == "__main__":
    unittest.main()
