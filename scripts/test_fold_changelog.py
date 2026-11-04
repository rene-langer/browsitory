#!/usr/bin/env python3
"""Tests for fold-changelog.py, with real temporary git repositories."""

import importlib.util
import io
import os
import pathlib
import subprocess
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest import mock

MODULE_PATH = pathlib.Path(__file__).resolve().parent / "fold-changelog.py"
spec = importlib.util.spec_from_file_location("fold_changelog", MODULE_PATH)
fold_changelog = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(fold_changelog)

CHANGELOG = """# Changelog

Intro.

## [Unreleased]

### Older topic

- Fixed: older thing (#1).

## [0.3.0] - 2026-10-21

### Released topic

- Added: a thing.
"""


class FoldTextTests(unittest.TestCase):
    def test_inserts_fragments_at_the_top_of_unreleased_in_order(self):
        result = fold_changelog.fold_text(
            CHANGELOG,
            ["### New A\n\n- Fixed: a (#3).\n", "\n### New B\n\n- Added: b.\n\n"],
        )
        self.assertEqual(
            result,
            """# Changelog

Intro.

## [Unreleased]

### New A

- Fixed: a (#3).

### New B

- Added: b.

### Older topic

- Fixed: older thing (#1).

## [0.3.0] - 2026-10-21

### Released topic

- Added: a thing.
""",
        )

    def test_empty_unreleased_section(self):
        text = "# Changelog\n\n## [Unreleased]\n\n## [0.3.0] - 2026-10-21\n\n- x\n"
        self.assertEqual(
            fold_changelog.fold_text(text, ["### New\n\n- Fixed: y.\n"]),
            "# Changelog\n\n## [Unreleased]\n\n### New\n\n- Fixed: y.\n\n"
            "## [0.3.0] - 2026-10-21\n\n- x\n",
        )

    def test_unreleased_at_end_of_file(self):
        self.assertEqual(
            fold_changelog.fold_text("## [Unreleased]\n", ["### New\n\n- y\n"]),
            "## [Unreleased]\n\n### New\n\n- y\n",
        )

    def test_no_fragments_leaves_text_unchanged(self):
        self.assertEqual(fold_changelog.fold_text(CHANGELOG, []), CHANGELOG)

    def test_refuses_without_unreleased(self):
        with self.assertRaises(ValueError):
            fold_changelog.fold_text(
                "# Changelog\n\n## [0.3.0] - 2026-10-21\n", ["### New\n\n- y\n"]
            )

    def test_fragment_already_in_unreleased_is_not_added_twice(self):
        once = fold_changelog.fold_text(CHANGELOG, ["### New\n\n- Fixed: a.\n"])
        self.assertEqual(
            fold_changelog.fold_text(once, ["### New\n\n- Fixed: a.\n"]), once
        )


class OrderTests(unittest.TestCase):
    def test_newest_first_then_filename_then_untracked_on_top(self):
        added_at = {
            "a.md": 100,
            "b.md": 300,
            "c.md": 200,
            "d.md": 200,
            "x-untracked.md": None,
            "w-untracked.md": None,
        }
        self.assertEqual(
            fold_changelog.order_fragments(added_at),
            ["w-untracked.md", "x-untracked.md", "b.md", "c.md", "d.md", "a.md"],
        )


class RepoTestCase(unittest.TestCase):
    """A temporary git repository with CHANGELOG.md, isolated from user config."""

    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = pathlib.Path(tmp.name) / "repo"
        self.root.mkdir()
        home = pathlib.Path(tmp.name) / "home"
        home.mkdir()
        env = {
            "HOME": str(home),
            "GIT_CONFIG_GLOBAL": str(home / "gitconfig"),
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CEILING_DIRECTORIES": tmp.name,
            "GIT_AUTHOR_NAME": "Test",
            "GIT_AUTHOR_EMAIL": "test@example.com",
            "GIT_COMMITTER_NAME": "Test",
            "GIT_COMMITTER_EMAIL": "test@example.com",
        }
        patcher = mock.patch.dict(os.environ, env)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.git("init", "-q")
        (self.root / "CHANGELOG.md").write_text(CHANGELOG)
        (self.root / "changelog.d").mkdir()
        (self.root / "changelog.d" / "README.md").write_text("# Fragments\n")
        self.commit("base", 1_700_000_000)

    def git(self, *args, env=None):
        subprocess.run(
            ["git", *args],
            cwd=self.root,
            check=True,
            capture_output=True,
            env={**os.environ, **(env or {})},
        )

    def commit(self, message, timestamp):
        date = f"{timestamp} +0000"
        self.git("add", "-A")
        self.git(
            "commit",
            "-q",
            "-m",
            message,
            env={"GIT_AUTHOR_DATE": date, "GIT_COMMITTER_DATE": date},
        )

    def write_fragment(self, name, heading):
        (self.root / "changelog.d" / name).write_text(
            f"### {heading}\n\n- Fixed: {heading.lower()}.\n"
        )

    def run_main(self, *args):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = fold_changelog.main(["--root", str(self.root), *args])
        return code, out.getvalue(), err.getvalue()

    def changelog(self):
        return (self.root / "CHANGELOG.md").read_text()

    def fragments(self):
        return sorted(p.name for p in (self.root / "changelog.d").iterdir())


class FoldRepoTests(RepoTestCase):
    def test_folds_newest_commit_first_and_deletes_fragments(self):
        self.write_fragment("300-old.md", "Old")
        self.commit("old", 1_700_000_100)
        self.write_fragment("100-new.md", "New")
        self.commit("new", 1_700_000_200)
        self.write_fragment("200-untracked.md", "Untracked")

        code, _, err = self.run_main()

        self.assertEqual(code, 0, err)
        text = self.changelog()
        self.assertLess(text.index("### Untracked"), text.index("### New"))
        self.assertLess(text.index("### New"), text.index("### Old"))
        self.assertLess(text.index("### Old"), text.index("### Older topic"))
        self.assertEqual(self.fragments(), ["README.md"])

    def test_second_run_changes_nothing(self):
        self.write_fragment("1-a.md", "A")
        self.assertEqual(self.run_main()[0], 0)
        folded = self.changelog()

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertIn("no fragments", out)
        self.assertEqual(self.changelog(), folded)

    def test_refuses_without_unreleased_and_keeps_fragments(self):
        (self.root / "CHANGELOG.md").write_text("# Changelog\n\n## [0.3.0] - x\n")
        self.write_fragment("1-a.md", "A")

        code, _, err = self.run_main()

        self.assertEqual(code, 1)
        self.assertIn("[Unreleased]", err)
        self.assertEqual(self.fragments(), ["1-a.md", "README.md"])
        self.assertEqual(self.changelog(), "# Changelog\n\n## [0.3.0] - x\n")

    def test_refuses_malformed_fragment_and_changes_nothing(self):
        self.write_fragment("1-a.md", "A")
        (self.root / "changelog.d" / "2-bad.md").write_text("- Fixed: no heading.\n")

        code, _, err = self.run_main()

        self.assertEqual(code, 1)
        self.assertIn("2-bad.md", err)
        self.assertEqual(self.changelog(), CHANGELOG)
        self.assertEqual(self.fragments(), ["1-a.md", "2-bad.md", "README.md"])

    def test_ignores_readme_and_other_files(self):
        (self.root / "changelog.d" / "notes.txt").write_text("not a fragment\n")

        code, out, _ = self.run_main()

        self.assertEqual(code, 0)
        self.assertIn("no fragments", out)
        self.assertEqual(self.changelog(), CHANGELOG)

    def test_works_outside_a_git_repository(self):
        # No history: every fragment counts as untracked, ordered by file name.
        (self.root / ".git").rename(self.root / "not-git")
        self.write_fragment("2-b.md", "B")
        self.write_fragment("1-a.md", "A")

        self.assertEqual(self.run_main()[0], 0)

        text = self.changelog()
        self.assertLess(text.index("### A"), text.index("### B"))


class CheckModeTests(RepoTestCase):
    def test_check_passes_without_fragments(self):
        code, _, _ = self.run_main("--check")
        self.assertEqual(code, 0)

    def test_check_fails_with_fragments_and_changes_nothing(self):
        self.write_fragment("1-a.md", "A")

        code, _, err = self.run_main("--check")

        self.assertEqual(code, 1)
        self.assertIn("changelog.d/1-a.md", err)
        self.assertEqual(self.changelog(), CHANGELOG)
        self.assertEqual(self.fragments(), ["1-a.md", "README.md"])


if __name__ == "__main__":
    unittest.main()
