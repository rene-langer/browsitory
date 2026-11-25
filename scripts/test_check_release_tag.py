#!/usr/bin/env python3
"""Tests for check-release-tag.py, with real temporary git repositories and a faked `gh`."""

import importlib.util
import io
import pathlib
import subprocess
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

MODULE_PATH = pathlib.Path(__file__).resolve().parent / "check-release-tag.py"
spec = importlib.util.spec_from_file_location("check_release_tag", MODULE_PATH)
check_release_tag = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(check_release_tag)

RELEASED = """# Changelog

Intro.

## [Unreleased]

## [0.17.0] - 2026-11-30

### Topic

- Fixed: a thing (#1).

## [0.16.0] - 2026-11-24

### Older topic

- Added: another thing.
"""

# docs/RELEASING.md step 2 forgotten: the changes are still under [Unreleased].
NOT_RENAMED = """# Changelog

## [Unreleased]

### Topic

- Fixed: a thing (#1).

## [0.16.0] - 2026-11-24

### Older topic

- Added: another thing.
"""


class ParseTagTests(unittest.TestCase):
    def test_returns_the_version_of_a_release_tag(self):
        self.assertEqual(check_release_tag.parse_tag("release/0.17.0"), "0.17.0")
        self.assertEqual(check_release_tag.parse_tag("release/10.0.12"), "10.0.12")

    def test_rejects_anything_but_release_major_minor_patch(self):
        for tag in (
            "release/0.17",
            "release/v0.17.0",
            "release/0.17.0-rc.1",
            "release/0.17.0.1",
            "release/01.17.0",
            "release/0.017.0",
            "release/dry-run",
            "0.17.0",
            "v0.17.0",
            "",
        ):
            with self.subTest(tag=tag):
                self.assertIsNone(check_release_tag.parse_tag(tag))


class ChangelogProblemsTests(unittest.TestCase):
    def test_a_released_version_directly_under_an_empty_unreleased_passes(self):
        self.assertEqual(check_release_tag.changelog_problems(RELEASED, "0.17.0"), [])

    def test_crlf_line_endings_pass_too(self):
        text = RELEASED.replace("\n", "\r\n")
        self.assertEqual(check_release_tag.changelog_problems(text, "0.17.0"), [])

    def test_changes_left_under_unreleased_fail(self):
        problems = check_release_tag.changelog_problems(NOT_RENAMED, "0.17.0")
        self.assertTrue(problems)
        self.assertTrue(any("[Unreleased]" in p for p in problems), problems)

    def test_a_missing_version_section_fails_and_names_the_version(self):
        problems = check_release_tag.changelog_problems(NOT_RENAMED, "0.17.0")
        self.assertTrue(any("0.17.0" in p for p in problems), problems)

    def test_a_section_for_another_version_fails(self):
        problems = check_release_tag.changelog_problems(RELEASED, "0.18.0")
        self.assertEqual(len(problems), 1, problems)
        self.assertIn("0.18.0", problems[0])
        self.assertIn("0.17.0", problems[0])

    def test_the_version_must_be_the_first_release_after_unreleased(self):
        problems = check_release_tag.changelog_problems(RELEASED, "0.16.0")
        self.assertEqual(len(problems), 1, problems)
        self.assertIn("0.16.0", problems[0])

    def test_a_missing_unreleased_heading_fails(self):
        text = RELEASED.replace("## [Unreleased]\n\n", "")
        problems = check_release_tag.changelog_problems(text, "0.17.0")
        self.assertEqual(len(problems), 1, problems)
        self.assertIn("[Unreleased]", problems[0])

    def test_an_invalid_date_fails(self):
        for heading in (
            "## [0.17.0] - 2026-13-01",
            "## [0.17.0] - 2026-11-31",
            "## [0.17.0] - YYYY-MM-DD",
            "## [0.17.0]",
            "## [0.17.0] 2026-11-30",
        ):
            with self.subTest(heading=heading):
                text = RELEASED.replace("## [0.17.0] - 2026-11-30", heading)
                self.assertEqual(len(check_release_tag.changelog_problems(text, "0.17.0")), 1)

    def test_an_empty_version_section_fails(self):
        text = RELEASED.replace("### Topic\n\n- Fixed: a thing (#1).\n\n", "")
        problems = check_release_tag.changelog_problems(text, "0.17.0")
        self.assertEqual(len(problems), 1, problems)
        self.assertIn("0.17.0", problems[0])

    def test_a_version_that_only_starts_with_the_tag_version_fails(self):
        text = RELEASED.replace("[0.17.0]", "[0.17.01]")
        self.assertEqual(len(check_release_tag.changelog_problems(text, "0.17.0")), 1)


class CiProblemsTests(unittest.TestCase):
    def run_(self, conclusion, status="completed", branch="main", event="push"):
        return {"conclusion": conclusion, "status": status, "headBranch": branch, "event": event}

    def test_a_successful_run_on_main_passes(self):
        self.assertEqual(check_release_tag.ci_problems([self.run_("success")]), [])

    def test_one_successful_run_among_failed_ones_passes(self):
        runs = [self.run_("failure"), self.run_("success", event="schedule")]
        self.assertEqual(check_release_tag.ci_problems(runs), [])

    def test_no_run_fails(self):
        self.assertEqual(len(check_release_tag.ci_problems([])), 1)

    def test_a_failed_run_fails(self):
        problems = check_release_tag.ci_problems([self.run_("failure")])
        self.assertEqual(len(problems), 1)
        self.assertIn("failure", problems[0])

    def test_a_run_still_in_progress_fails_and_says_to_rerun(self):
        problems = check_release_tag.ci_problems([self.run_("", status="in_progress")])
        self.assertEqual(len(problems), 1)
        self.assertIn("re-run", problems[0])

    def test_a_successful_run_on_another_branch_does_not_count(self):
        runs = [self.run_("success", branch="feature", event="pull_request")]
        self.assertEqual(len(check_release_tag.ci_problems(runs)), 1)


def git(cwd, *args):
    return subprocess.run(
        ["git", *args], cwd=cwd, check=True, capture_output=True, text=True
    ).stdout.strip()


class GitRepoTestCase(unittest.TestCase):
    """A repository with main (two commits) and a side branch off its first commit.

    origin/main is a remote-tracking ref at main's tip, as actions/checkout leaves it.
    """

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.root = pathlib.Path(self._tmp.name) / "repo"
        self.root.mkdir()
        git(self.root, "init", "-q", "-b", "main")
        git(self.root, "config", "user.email", "test@example.com")
        git(self.root, "config", "user.name", "Test")
        git(self.root, "config", "commit.gpgsign", "false")
        self.first = self.commit(NOT_RENAMED, "work")
        self.release = self.commit(RELEASED, "release prep")
        git(self.root, "update-ref", "refs/remotes/origin/main", self.release)
        git(self.root, "checkout", "-q", "-b", "side", self.first)
        self.side = self.commit(RELEASED, "release prep off main")
        git(self.root, "checkout", "-q", "main")

    def commit(self, changelog, message):
        (self.root / "CHANGELOG.md").write_text(changelog)
        git(self.root, "add", "CHANGELOG.md")
        git(self.root, "commit", "-q", "-m", message)
        return git(self.root, "rev-parse", "HEAD")


class IsOnMainTests(GitRepoTestCase):
    def test_a_commit_on_main_is_on_main(self):
        self.assertTrue(check_release_tag.is_on_main(self.root, self.release, "origin/main"))
        self.assertTrue(check_release_tag.is_on_main(self.root, self.first, "origin/main"))

    def test_a_commit_only_on_another_branch_is_not(self):
        self.assertFalse(check_release_tag.is_on_main(self.root, self.side, "origin/main"))

    def test_a_missing_main_ref_raises(self):
        with self.assertRaises(check_release_tag.CheckError):
            check_release_tag.is_on_main(self.root, self.release, "origin/nope")

    def test_a_shallow_clone_raises(self):
        clone = pathlib.Path(self._tmp.name) / "shallow"
        subprocess.run(
            ["git", "clone", "-q", "--depth", "1", f"file://{self.root}", str(clone)],
            check=True,
            capture_output=True,
        )
        head = git(clone, "rev-parse", "HEAD")
        with self.assertRaises(check_release_tag.CheckError) as raised:
            check_release_tag.is_on_main(clone, head, "origin/main")
        self.assertIn("shallow", str(raised.exception))


class MainTests(GitRepoTestCase):
    def setUp(self):
        super().setUp()
        self.gh_calls = []
        self.runs = [{"conclusion": "success", "status": "completed", "headBranch": "main", "event": "push"}]

    def list_ci_runs(self, sha, repo):
        self.gh_calls.append((sha, repo))
        return self.runs

    def main(self, *argv):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = check_release_tag.main(
                ["--root", str(self.root), *argv], list_ci_runs=self.list_ci_runs
            )
        return code, out.getvalue(), err.getvalue()

    def test_a_good_release_commit_passes(self):
        code, out, err = self.main("--tag", "release/0.17.0", "--commit", self.release)
        self.assertEqual(code, 0, err)
        self.assertEqual(self.gh_calls, [(self.release, None)])

    def test_the_commit_defaults_to_head_and_is_passed_to_gh_as_a_full_sha(self):
        code, _, err = self.main("--tag", "release/0.17.0", "--repo", "owner/name")
        self.assertEqual(code, 0, err)
        self.assertEqual(self.gh_calls, [(self.release, "owner/name")])

    def test_a_bad_tag_fails_before_anything_else(self):
        code, _, err = self.main("--tag", "release/0.17", "--commit", self.release)
        self.assertEqual(code, 1)
        self.assertIn("release/0.17", err)
        self.assertEqual(self.gh_calls, [])

    def test_the_changelog_is_read_from_the_commit_not_the_working_tree(self):
        (self.root / "CHANGELOG.md").write_text(NOT_RENAMED)
        code, _, err = self.main("--tag", "release/0.17.0", "--commit", self.release)
        self.assertEqual(code, 0, err)

    def test_a_commit_without_the_version_section_fails(self):
        code, _, err = self.main("--tag", "release/0.17.0", "--commit", self.first)
        self.assertEqual(code, 1)
        self.assertIn("0.17.0", err)

    def test_a_commit_off_main_fails(self):
        code, _, err = self.main("--tag", "release/0.17.0", "--commit", self.side)
        self.assertEqual(code, 1)
        self.assertIn("main", err)

    def test_a_failed_ci_run_fails(self):
        self.runs = [{"conclusion": "failure", "status": "completed", "headBranch": "main", "event": "push"}]
        code, _, err = self.main("--tag", "release/0.17.0", "--commit", self.release)
        self.assertEqual(code, 1)
        self.assertIn("ci.yml", err)

    def test_every_problem_is_reported_at_once(self):
        self.runs = []
        code, _, err = self.main("--tag", "release/0.18.0", "--commit", self.side)
        self.assertEqual(code, 1)
        self.assertIn("0.18.0", err)
        self.assertIn("main", err)
        self.assertIn("ci.yml", err)

    def test_an_unknown_commit_fails_with_an_error(self):
        code, _, err = self.main("--tag", "release/0.17.0", "--commit", "0" * 40)
        self.assertEqual(code, 1)
        self.assertTrue(err)


if __name__ == "__main__":
    unittest.main()
