#!/usr/bin/env python3
"""Tests for report-mutants.py, on hand-made `mutants.out` folders."""

import importlib.util
import pathlib
import tempfile
import unittest

MODULE_PATH = pathlib.Path(__file__).resolve().parent / "report-mutants.py"
spec = importlib.util.spec_from_file_location("report_mutants", MODULE_PATH)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)

RUN_URL = "https://example.invalid/run/1"
SHA = "0123abc"
CLAIM = "Every viable mutant was caught by a test."
DIDNT_RUN = "Mutation testing didn't run"


class ReportMutantsTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.out = pathlib.Path(self.tmp.name) / "mutants.out"

    def tearDown(self):
        self.tmp.cleanup()

    def results(self, **lists):
        """Writes `mutants.out` with the given lists (name -> lines)."""
        self.out.mkdir()
        for name, lines in lists.items():
            (self.out / f"{name}.txt").write_text("".join(f"{line}\n" for line in lines))

    def report(self, outcome, status, relevant="true", comment=True):
        return module.report(
            relevant=relevant,
            outcome=outcome,
            status=status,
            out_dir=self.out,
            run_url=RUN_URL,
            head_sha=SHA,
            comment=comment,
        )

    def test_an_earlier_failed_step_says_mutation_testing_didnt_run(self):
        # The apt install failed: the mutants step was skipped, no `mutants.out` (#798).
        for comment in (True, False):
            text = self.report(outcome="skipped", status="", comment=comment)
            self.assertIn(DIDNT_RUN, text)
            self.assertIn("an earlier step", text)
            self.assertIn(RUN_URL, text)
            self.assertNotIn(CLAIM, text)
            self.assertNotIn("| Caught |", text)
            self.assertNotIn("To triage", text)

    def test_a_step_that_never_started_reports_like_a_skipped_one(self):
        # A cancelled or never-evaluated step leaves the outcome empty.
        text = self.report(outcome="", status="")
        self.assertIn(DIDNT_RUN, text)
        self.assertNotIn(CLAIM, text)

    def test_cargo_mutants_failing_on_its_own_says_so(self):
        # Exit 1 (usage) or 70 (internal error): nothing was mutated.
        text = self.report(outcome="success", status="1")
        self.assertIn(DIDNT_RUN, text)
        self.assertIn("exit status 1", text)
        self.assertNotIn(CLAIM, text)
        self.assertNotIn("| Caught |", text)

    def test_no_results_folder_is_not_a_success(self):
        text = self.report(outcome="success", status="0")
        self.assertIn(DIDNT_RUN, text)
        self.assertIn("no results", text)
        self.assertNotIn(CLAIM, text)

    def test_a_failed_baseline_says_no_mutant_was_tested(self):
        self.results(caught=[], missed=[])
        text = self.report(outcome="success", status="4")
        self.assertIn("The tests failed without any mutation", text)
        self.assertNotIn(CLAIM, text)
        self.assertNotIn("To triage", text)

    def test_the_time_limit_keeps_the_partial_counts(self):
        self.results(caught=["a", "b"], missed=["src/x.rs:1: replace f -> bool with true"])
        text = self.report(outcome="failure", status="")
        self.assertIn("| 2 | 1 | 0 | 0 |", text)
        self.assertIn("time limit", text)
        self.assertIn("replace f -> bool with true", text)
        self.assertIn("To triage", text)

    def test_all_caught(self):
        self.results(caught=["a", "b", "c"], unviable=["d"])
        text = self.report(outcome="success", status="0")
        self.assertIn("| 3 | 0 | 0 | 1 |", text)
        self.assertIn(CLAIM, text)
        self.assertNotIn(DIDNT_RUN, text)

    def test_no_mutants_in_the_changed_code(self):
        self.results()
        text = self.report(outcome="success", status="0")
        self.assertIn("No mutants were generated", text)
        self.assertNotIn(CLAIM, text)

    def test_missed_and_timed_out_mutants_are_listed(self):
        self.results(caught=["a"], missed=["m1", "m2"], timeout=["t1"])
        text = self.report(outcome="success", status="2")
        self.assertIn("| 1 | 2 | 1 | 0 |", text)
        self.assertIn("### Missed: no test noticed these changes (2)", text)
        self.assertIn("### Timed out (1)", text)
        self.assertNotIn(CLAIM, text)

    def test_a_long_list_is_cut_and_points_at_the_artifact(self):
        self.results(missed=[f"m{i}" for i in range(150)])
        text = self.report(outcome="success", status="2")
        self.assertIn("m99\n", text)
        self.assertNotIn("m100\n", text)
        self.assertIn("First 100 of 150", text)

    def test_nothing_relevant(self):
        text = self.report(outcome="", status="", relevant="false")
        self.assertIn("nothing to mutate", text)
        self.assertIn(SHA, text)
        self.assertNotIn(DIDNT_RUN, text)

    def test_the_comment_has_the_marker_and_the_summary_does_not(self):
        self.results(caught=["a"])
        comment = self.report(outcome="success", status="0", comment=True)
        summary = self.report(outcome="success", status="0", comment=False)
        self.assertTrue(comment.startswith(module.MARKER))
        self.assertNotIn(module.MARKER, summary)
        self.assertIn("## Mutation testing (changed code)", summary)


if __name__ == "__main__":
    unittest.main()
