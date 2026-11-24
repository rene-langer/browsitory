#!/usr/bin/env python3
"""Tests for check-changelog.py's pure changelog-required predicate."""

import contextlib
import importlib.util
import io
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

MODULE_PATH = pathlib.Path(__file__).resolve().parent / "check-changelog.py"
spec = importlib.util.spec_from_file_location("check_changelog", MODULE_PATH)
check_changelog = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(check_changelog)

MANIFESTS = (
    "extension/package.json",
    "extension/e2e/package.json",
    "e2e/package.json",
    "frontend/package.json",
)


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

    def test_watched_extension_file_without_changelog(self):
        # The extension host ships in every VSIX (#1057).
        self.assertTrue(
            check_changelog.needs_changelog(["extension/src/sidecarBridge.ts"])
        )

    def test_watched_extension_script_without_changelog(self):
        self.assertTrue(
            check_changelog.needs_changelog(["extension/scripts/package-vsix.mjs"])
        )

    def test_watched_extension_e2e_file_without_changelog(self):
        # Like e2e/, the extension's E2E tests need an entry too (#1057).
        self.assertTrue(
            check_changelog.needs_changelog(
                ["extension/e2e/src/specs/first-flow.spec.ts"]
            )
        )

    def test_extension_file_with_fragment(self):
        self.assertFalse(
            check_changelog.needs_changelog(
                ["extension/src/sidecarBridge.ts", "changelog.d/1057-extension.md"]
            )
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
                    "extension/package.json",
                    "extension/pnpm-lock.yaml",
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


class PackageJsonFieldTests(unittest.TestCase):
    """changed_beyond_dependencies compares two package.json texts (#1094)."""

    OLD = (
        '{"name": "browsitory", "version": "0.1.0",'
        ' "engines": {"vscode": "^1.134.0"},'
        ' "contributes": {"commands": [{"command": "browsitory.open"}]},'
        ' "scripts": {"compile": "tsc"},'
        ' "dependencies": {"a": "^1.0.0"},'
        ' "devDependencies": {"b": "^2.0.0"},'
        ' "optionalDependencies": {"c": "^3.0.0"},'
        ' "peerDependencies": {"d": "^4.0.0"},'
        ' "packageManager": "pnpm@12.23.0",'
        ' "pnpm": {"overrides": {"e": "1.0.0"}}}'
    )

    def changed(self, **fields):
        new = json.loads(self.OLD)
        new.update(fields)
        return check_changelog.changed_beyond_dependencies(
            self.OLD, json.dumps(new)
        )

    def test_unchanged(self):
        self.assertFalse(
            check_changelog.changed_beyond_dependencies(self.OLD, self.OLD)
        )

    def test_dependency_field_bumps_are_exempt(self):
        self.assertFalse(self.changed(dependencies={"a": "^1.1.0"}))
        self.assertFalse(self.changed(devDependencies={"b": "^2.0.1", "f": "1"}))
        self.assertFalse(self.changed(optionalDependencies={"c": "^3.1.0"}))
        self.assertFalse(self.changed(peerDependencies={"d": "^4.1.0"}))
        self.assertFalse(self.changed(packageManager="pnpm@12.24.0"))
        self.assertFalse(self.changed(pnpm={"overrides": {"e": "1.0.1"}}))

    def test_reformatting_alone_is_exempt(self):
        reformatted = json.dumps(json.loads(self.OLD), indent=4, sort_keys=True)
        self.assertFalse(
            check_changelog.changed_beyond_dependencies(self.OLD, reformatted)
        )

    def test_contributes_change_counts(self):
        self.assertTrue(
            self.changed(contributes={"commands": [{"command": "browsitory.log"}]})
        )

    def test_other_field_changes_count(self):
        self.assertTrue(self.changed(scripts={"compile": "tsc -b"}))
        self.assertTrue(self.changed(engines={"vscode": "^1.140.0"}))
        self.assertTrue(self.changed(displayName="Browsitory"))

    def test_removed_field_counts(self):
        new = json.loads(self.OLD)
        del new["contributes"]
        self.assertTrue(
            check_changelog.changed_beyond_dependencies(self.OLD, json.dumps(new))
        )

    def test_added_or_deleted_file_counts(self):
        self.assertTrue(check_changelog.changed_beyond_dependencies(None, self.OLD))
        self.assertTrue(check_changelog.changed_beyond_dependencies(self.OLD, None))

    def test_invalid_json_counts(self):
        self.assertTrue(check_changelog.changed_beyond_dependencies(self.OLD, "{"))
        self.assertTrue(check_changelog.changed_beyond_dependencies("[1]", "[2]"))

    def test_manifest_changed_beyond_dependencies_needs_changelog(self):
        self.assertTrue(
            check_changelog.needs_changelog(
                ["extension/package.json", "extension/pnpm-lock.yaml"],
                manifests_beyond_dependencies={"extension/package.json"},
            )
        )

    def test_manifest_changed_beyond_dependencies_with_fragment(self):
        self.assertFalse(
            check_changelog.needs_changelog(
                ["extension/package.json", "changelog.d/1094-contributes.md"],
                manifests_beyond_dependencies={"extension/package.json"},
            )
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
    """parse_name_status reads `git diff -z --name-status` output."""

    def test_deleted_fragments_are_dropped_but_other_deletions_kept(self):
        output = (
            "M\0CHANGELOG.md\0"
            "D\0changelog.d/240-a.md\0"
            "D\0crates/git-core/src/old.rs\0"
            "A\0changelog.d/241-b.md\0"
        )
        self.assertEqual(
            check_changelog.parse_name_status(output),
            ["CHANGELOG.md", "crates/git-core/src/old.rs", "changelog.d/241-b.md"],
        )

    def test_deleting_a_fragment_does_not_count_as_an_entry(self):
        files = check_changelog.parse_name_status(
            "M\0crates/git-core/src/lib.rs\0D\0changelog.d/240-a.md\0"
        )
        self.assertTrue(check_changelog.needs_changelog(files))

    def test_paths_with_tabs_and_non_ascii_are_kept_verbatim(self):
        self.assertEqual(
            check_changelog.parse_name_status(
                "A\0changelog.d/240-café.md\0M\0crates/a\tb.rs\0"
            ),
            ["changelog.d/240-café.md", "crates/a\tb.rs"],
        )


class StrayFileTests(unittest.TestCase):
    def test_other_files_directly_in_the_fragment_folder_are_stray(self):
        self.assertEqual(
            check_changelog.stray_files(
                [
                    "changelog.d/240-a.MD",
                    "changelog.d/240-a.txt",
                    "changelog.d/.md",
                    "changelog.d/README.md",
                    "changelog.d/241-b.md",
                    "changelog.d/old/242-c.md",
                    "crates/git-core/src/lib.rs",
                ]
            ),
            [
                "changelog.d/240-a.MD",
                "changelog.d/240-a.txt",
                "changelog.d/.md",
            ],
        )


class RepoTests(unittest.TestCase):
    """changed_files and default_base against a real temporary git repository."""

    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = pathlib.Path(tmp.name) / "repo"
        self.root.mkdir()
        home = pathlib.Path(tmp.name) / "home"
        home.mkdir()
        patcher = mock.patch.dict(
            os.environ,
            {
                "HOME": str(home),
                "GIT_CONFIG_GLOBAL": str(home / "gitconfig"),
                "GIT_CONFIG_NOSYSTEM": "1",
                "GIT_CEILING_DIRECTORIES": tmp.name,
                "GIT_AUTHOR_NAME": "Test",
                "GIT_AUTHOR_EMAIL": "test@example.com",
                "GIT_COMMITTER_NAME": "Test",
                "GIT_COMMITTER_EMAIL": "test@example.com",
            },
        )
        patcher.start()
        self.addCleanup(patcher.stop)
        cwd = os.getcwd()
        os.chdir(self.root)
        self.addCleanup(os.chdir, cwd)
        self.git("init", "-q", "-b", "main")
        (self.root / "base.txt").write_text("base\n")
        self.commit("base")

    def git(self, *args):
        return subprocess.run(
            ["git", *args], check=True, capture_output=True, text=True
        ).stdout

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", message)

    def test_non_ascii_fragment_name_is_found(self):
        self.git("checkout", "-q", "-b", "topic")
        (self.root / "changelog.d").mkdir()
        (self.root / "changelog.d" / "240-café.md").write_text("### Café\n\n- x\n")
        self.commit("fragment")
        files = check_changelog.changed_files("main")
        self.assertEqual(files, ["changelog.d/240-café.md"])
        self.assertEqual(
            check_changelog.fragment_files(files), ["changelog.d/240-café.md"]
        )

    def write_json(self, path, data):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps(data, indent=2) + "\n")

    def commit_manifests(self):
        """Commit MANIFESTS on main, then branch off to topic."""
        self.manifest = {
            "name": "browsitory",
            "contributes": {"commands": [{"command": "browsitory.open"}]},
            "scripts": {"compile": "tsc"},
            "devDependencies": {"typescript": "~6.0.2"},
            "packageManager": "pnpm@12.23.0",
        }
        for path in MANIFESTS:
            self.write_json(path, self.manifest)
        self.commit("manifests")
        self.git("checkout", "-q", "-b", "topic")

    def run_main(self):
        stderr = io.StringIO()
        argv = ["check-changelog.py", "--base", "main"]
        with (
            mock.patch.object(sys, "argv", argv),
            mock.patch.dict(os.environ, {"SKIP_CHANGELOG_CHECK": ""}),
            contextlib.redirect_stderr(stderr),
        ):
            return check_changelog.main(), stderr.getvalue()

    def test_contributes_change_needs_an_entry(self):
        self.commit_manifests()
        self.write_json(
            "extension/package.json",
            {**self.manifest, "contributes": {"commands": []}},
        )
        self.commit("drop a command")
        code, stderr = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("no changelog entry", stderr)
        files = check_changelog.changed_files("main")
        self.assertEqual(
            check_changelog.manifests_changed_beyond_dependencies("main", files),
            {"extension/package.json"},
        )

    def test_contributes_change_with_fragment_passes(self):
        self.commit_manifests()
        self.write_json(
            "extension/package.json",
            {**self.manifest, "contributes": {"commands": []}},
        )
        (self.root / "changelog.d").mkdir()
        (self.root / "changelog.d" / "1094-x.md").write_text("### X\n\n- x\n")
        self.commit("drop a command, with a fragment")
        self.assertEqual(self.run_main(), (0, ""))

    def test_dependency_only_changes_pass(self):
        self.commit_manifests()
        for path in MANIFESTS:
            self.write_json(
                path,
                {
                    **self.manifest,
                    "devDependencies": {"typescript": "~6.0.3", "x": "^1.0.0"},
                    "packageManager": "pnpm@12.24.0",
                },
            )
        self.commit("bump dependencies")
        self.assertEqual(self.run_main(), (0, ""))
        files = check_changelog.changed_files("main")
        self.assertEqual(
            check_changelog.manifests_changed_beyond_dependencies("main", files),
            set(),
        )

    def test_scripts_change_in_a_watched_manifest_needs_an_entry(self):
        self.commit_manifests()
        self.write_json("e2e/package.json", {**self.manifest, "scripts": {}})
        self.commit("change e2e scripts")
        self.assertEqual(self.run_main()[0], 1)

    def test_frontend_manifest_is_not_watched(self):
        # frontend/package.json is outside the watched frontend/src/ (#1094).
        self.commit_manifests()
        self.write_json("frontend/package.json", {**self.manifest, "scripts": {}})
        self.commit("change frontend scripts")
        self.assertEqual(self.run_main(), (0, ""))

    def test_every_watched_manifest_is_compared(self):
        self.commit_manifests()
        for path in MANIFESTS:
            self.write_json(path, {**self.manifest, "scripts": {"compile": "tsc -b"}})
        self.commit("change scripts")
        files = check_changelog.changed_files("main")
        self.assertEqual(
            check_changelog.manifests_changed_beyond_dependencies("main", files),
            {
                "extension/package.json",
                "extension/e2e/package.json",
                "e2e/package.json",
            },
        )

    def test_added_and_deleted_watched_manifests_count(self):
        self.commit_manifests()
        (self.root / "e2e" / "package.json").unlink()
        self.write_json("extension/new/package.json", self.manifest)
        self.commit("move a project")
        self.assertEqual(self.run_main()[0], 1)
        files = check_changelog.changed_files("main")
        self.assertEqual(
            check_changelog.manifests_changed_beyond_dependencies("main", files),
            {"e2e/package.json", "extension/new/package.json"},
        )

    def test_default_base_is_main_without_origin(self):
        self.assertEqual(check_changelog.default_base(), "main")

    def test_default_base_prefers_origin_main(self):
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")
        self.assertEqual(check_changelog.default_base(), "origin/main")


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
