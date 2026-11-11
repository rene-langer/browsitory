#!/usr/bin/env python3
"""List the test assertions a diff removes or rewrites (#310).

Reads a unified diff (`git diff base...HEAD`) and prints a Markdown report of
every test file the diff takes an assertion line out of, with those lines. A
changed assertion shows as one removed and one added line, so a loosened
expectation is listed as well as a deleted one. The report is a prompt for
review, not a verdict: a deliberate behaviour change or a moved test shows up
too. Always exits 0 (CI's `removed-assertions` job is informational).

Usage: report-removed-assertions.py [DIFF_FILE]   (stdin without one)
"""

import re
import sys

MARKER = "<!-- removed-assertions -->"
MAX_LINES_PER_FILE = 30
MAX_LINE_CHARS = 200

# Rust: `assert!`, `assert_eq!`, `assert_ne!` (not `debug_assert!`, which is production code) in
# any crate file, inline `#[cfg(test)]` modules included. `matches!` inside an `assert!` is
# covered by the `assert!`.
RUST_ASSERT = re.compile(r"(?<![\w])assert(?:_eq|_ne)?!\s*[(\[{]")
# Vitest/Jest, Mocha/WebdriverIO and Node's assert: `expect(`, `assert(`, `assert.x(`,
# `assert.strictEqual(` and the like.
TS_ASSERT = re.compile(r"(?<![\w.])(?:expect|assert)\s*(?:\.\w+\s*)?\(")
# Python unittest and plain asserts.
PY_ASSERT = re.compile(r"(?:\bself\.assert\w*\s*\(|^\s*assert\b)")


def pattern_for(path):
    """The assertion pattern for a test file at `path`, or None when it isn't one."""
    if path.startswith("crates/") and path.endswith(".rs"):
        return RUST_ASSERT
    if re.search(r"\.(test|spec)\.(ts|tsx|js|mjs)$", path):
        return TS_ASSERT
    if re.search(r"(^|/)test_[^/]*\.py$", path):
        return PY_ASSERT
    return None


def removed_assertions(diff_text):
    """Maps each test file to `(removed, added)`: the assertion lines the diff removes, and how
    many it adds, in diff order. Files without a removed assertion are left out."""
    files = {}
    path = None
    pattern = None
    in_hunk = False
    for line in diff_text.splitlines():
        if line.startswith("diff --git "):
            path, pattern, in_hunk = None, None, False
        elif line.startswith("@@"):
            in_hunk = True
        elif not in_hunk:
            # `--- a/x` names the old file, `+++ b/x` the new one, `/dev/null` a missing one (a
            # deleted file has only the first). The prefix is one letter and a slash: `a/`, `b/`,
            # or another with `diff.mnemonicPrefix`.
            match = re.match(r"(?:---|\+\+\+) [a-z]/(.+)$", line)
            if match:
                path = match.group(1)
                pattern = pattern_for(path)
        elif pattern is None:
            continue
        elif line.startswith("-") and pattern.search(line[1:]):
            files.setdefault(path, ([], [0]))[0].append(line[1:].strip())
        elif line.startswith("+") and pattern.search(line[1:]):
            files.setdefault(path, ([], [0]))[1][0] += 1
    return {name: (removed, added[0]) for name, (removed, added) in files.items() if removed}


def report(found):
    """The Markdown report (with `MARKER` first) for `removed_assertions`' result."""
    lines = [MARKER, "## Removed or changed test assertions", ""]
    if not found:
        lines.append("This diff removes or rewrites no assertion in a test file.")
        return "\n".join(lines) + "\n"
    lines += [
        "This diff takes these assertion lines out of test files (a changed one also shows as",
        "added). Check each is a deliberate behaviour change or a moved test, not a test made to",
        "pass; say which in the PR. See AGENTS.md, \"Task workflow\".",
        "",
        "| File | Removed | Added |",
        "|---|---|---|",
    ]
    for name, (removed, added) in sorted(found.items()):
        lines.append(f"| `{name}` | {len(removed)} | {added} |")
    lines.append("")
    for name, (removed, _) in sorted(found.items()):
        lines += [f"### `{name}`", "", "```"]
        lines += [text[:MAX_LINE_CHARS] for text in removed[:MAX_LINES_PER_FILE]]
        lines.append("```")
        if len(removed) > MAX_LINES_PER_FILE:
            lines.append(f"First {MAX_LINES_PER_FILE} of {len(removed)}.")
        lines.append("")
    return "\n".join(lines)


def main(argv):
    if len(argv) > 1:
        with open(argv[1], encoding="utf-8", errors="replace") as handle:
            diff_text = handle.read()
    else:
        diff_text = sys.stdin.read()
    sys.stdout.write(report(removed_assertions(diff_text)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
