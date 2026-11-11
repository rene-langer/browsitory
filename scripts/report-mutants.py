#!/usr/bin/env python3
"""Write the `mutants` job's report: the run summary or the pull request comment.

Reads cargo-mutants' results folder (`mutants.out`: caught.txt, missed.txt,
timeout.txt, unviable.txt, one mutant per line) and the job's state from the
environment, and prints Markdown:

  RELEVANT  `true` when the diff changes git-core or repo-service source
  OUTCOME   the mutation step's outcome (success, failure, skipped, or empty)
  STATUS    cargo-mutants' exit status, empty when the step didn't record one
  HEAD_SHA  the commit tested
  RUN_URL   the workflow run

Usage: report-mutants.py --comment|--summary [OUT_DIR]   (OUT_DIR: mutants.out)
"""

import os
import pathlib
import sys

MARKER = "<!-- mutants-report -->"
MAX_LINES = 100  # lines per list; with lines cut to 200 chars the body stays far below 65536
MAX_LINE_CHARS = 200


def lines_of(out_dir, name):
    path = pathlib.Path(out_dir) / name
    if not path.is_file():
        return []
    return [line for line in path.read_text().splitlines() if line]


def mutant_list(out_dir, name, heading, run_url):
    lines = lines_of(out_dir, name)
    if not lines:
        return []
    text = [f"### {heading} ({len(lines)})", "", "```"]
    text += [line[:MAX_LINE_CHARS] for line in lines[:MAX_LINES]]
    text.append("```")
    if len(lines) > MAX_LINES:
        text += [
            "",
            f"First {MAX_LINES} of {len(lines)}; the full list is in the `mutants-out` artifact "
            f"of [the run]({run_url}).",
        ]
    text.append("")
    return text


# cargo-mutants' exit statuses that mean it tested the mutants: all caught, some missed, some
# timed out. 4 is a failed baseline (no mutant tested); anything else is its own failure.
TESTED = ("0", "2", "3")
BASELINE_FAILED = "4"

TRIAGE = [
    "To triage: for each missed mutant add a test that fails on it, or say in the PR why it is",
    "equivalent or harmless. Mutants in `#[cfg(windows)]` files are always missed on this "
    "Linux runner.",
    "",
]


def not_run_reason(outcome, status, out_dir, run_url):
    """Why no mutant was tested, or None when cargo-mutants ran (to the end or its time limit)."""
    run = f"[the run]({run_url})"
    if outcome in ("", "skipped", "cancelled"):
        # The step never started: an earlier one failed (an apt or toolchain install, #798).
        return f"Mutation testing didn't run: an earlier step of the job failed. See {run}."
    if outcome == "failure" and status == "":
        return None  # cut off by the step's time limit; what finished is counted
    if status not in TESTED and status != BASELINE_FAILED:
        return f"Mutation testing didn't run: cargo-mutants stopped with exit status {status or 'unknown'}. See {run}."
    if not pathlib.Path(out_dir).is_dir():
        return f"Mutation testing didn't run: cargo-mutants left no results (no `mutants.out`). See {run}."
    return None


def report(*, relevant, outcome, status, out_dir, run_url, head_sha, comment):
    """The report's Markdown. `comment` adds the sticky comment's marker and footer."""
    text = [MARKER] if comment else []
    text += ["## Mutation testing (changed code)", ""]
    if relevant not in ("true", "false"):
        # The diff step itself didn't finish: whether there was anything to mutate is unknown.
        text += [f"Mutation testing didn't run: an earlier step of the job failed. See [the run]({run_url}).", ""]
    elif relevant == "false":
        text += [f"No Rust source changes in git-core or repo-service at {head_sha}; nothing to mutate.", ""]
    elif (reason := not_run_reason(outcome, status, out_dir, run_url)) is not None:
        # No table: zero counts would read as a clean run.
        text += [reason, ""]
    else:
        counts = [len(lines_of(out_dir, f"{name}.txt")) for name in ("caught", "missed", "timeout", "unviable")]
        text += [
            "| Caught | Missed | Timed out | Unviable |",
            "|---|---|---|---|",
            "| " + " | ".join(str(n) for n in counts) + " |",
            "",
        ]
        if status == BASELINE_FAILED:
            text += [
                "The tests failed without any mutation, so no mutant was tested. See the baseline in "
                f"[the run]({run_url})'s log and the `rust` job.",
                "",
            ]
        else:
            if outcome == "failure":
                text += ["The run hit its time limit; the counts cover only the mutants it finished.", ""]
            missed = mutant_list(out_dir, "missed.txt", "Missed: no test noticed these changes", run_url)
            timed_out = mutant_list(out_dir, "timeout.txt", "Timed out", run_url)
            text += missed + timed_out
            if sum(counts) == 0:
                text += ["No mutants were generated for the changed code.", ""]
            elif not missed and not timed_out:
                text += ["Every viable mutant was caught by a test.", ""]
            text += TRIAGE
    if comment:
        text.append(f"Informational, not a required check. [Run]({run_url}), commit {head_sha}.")
    return "\n".join(text) + "\n"


def main(argv):
    if len(argv) not in (2, 3) or argv[1] not in ("--comment", "--summary"):
        print(__doc__, file=sys.stderr)
        return 2
    out_dir = argv[2] if len(argv) == 3 else "mutants.out"
    sys.stdout.write(
        report(
            relevant=os.environ.get("RELEVANT", ""),
            outcome=os.environ.get("OUTCOME", ""),
            status=os.environ.get("STATUS", ""),
            out_dir=out_dir,
            run_url=os.environ.get("RUN_URL", ""),
            head_sha=os.environ.get("HEAD_SHA", ""),
            comment=argv[1] == "--comment",
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
