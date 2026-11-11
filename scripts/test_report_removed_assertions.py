#!/usr/bin/env python3
"""Tests for report-removed-assertions.py, on hand-written diffs."""

import importlib.util
import pathlib
import unittest

MODULE_PATH = pathlib.Path(__file__).resolve().parent / "report-removed-assertions.py"
spec = importlib.util.spec_from_file_location("report_removed_assertions", MODULE_PATH)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)


def file_diff(path, body, old="a", new="b"):
    return (
        f"diff --git {old}/{path} {new}/{path}\n"
        "index 1111111..2222222 100644\n"
        f"--- {old}/{path}\n"
        f"+++ {new}/{path}\n"
        "@@ -1,3 +1,3 @@\n"
        f"{body}"
    )


class RemovedAssertionsTests(unittest.TestCase):
    def found(self, diff):
        return module.removed_assertions(diff)

    def test_a_removed_rust_assertion_is_listed(self):
        diff = file_diff(
            "crates/git-core/tests/status.rs",
            " fn t() {\n-    assert_eq!(a, 1);\n+    let _ = a;\n }\n",
        )
        self.assertEqual(self.found(diff), {"crates/git-core/tests/status.rs": (["assert_eq!(a, 1);"], 0)})

    def test_a_changed_assertion_counts_as_removed_and_added(self):
        diff = file_diff(
            "crates/repo-service/src/worker/mod.rs",
            "-        assert!(x > 1);\n+        assert!(x > 0);\n",
        )
        self.assertEqual(self.found(diff), {"crates/repo-service/src/worker/mod.rs": (["assert!(x > 1);"], 1)})

    def test_debug_assert_and_added_only_assertions_are_not_listed(self):
        diff = file_diff(
            "crates/git-core/src/lib.rs",
            "-    debug_assert!(ok);\n+    assert!(ok);\n",
        )
        self.assertEqual(self.found(diff), {})

    def test_typescript_expect_and_assert_in_test_files(self):
        diff = file_diff(
            "frontend/src/App.test.tsx",
            "-    expect(undo).toHaveBeenCalled();\n-    assert.strictEqual(a, b);\n+    await x();\n",
        ) + file_diff(
            "extension/e2e/src/specs/undo.spec.ts",
            "-    assert(ok);\n",
        )
        self.assertEqual(
            self.found(diff),
            {
                "frontend/src/App.test.tsx": (["expect(undo).toHaveBeenCalled();", "assert.strictEqual(a, b);"], 0),
                "extension/e2e/src/specs/undo.spec.ts": (["assert(ok);"], 0),
            },
        )

    def test_non_test_typescript_is_ignored(self):
        diff = file_diff("frontend/src/App.tsx", "-    expect(x);\n")
        self.assertEqual(self.found(diff), {})

    def test_python_script_tests(self):
        diff = file_diff(
            "scripts/test_fold_changelog.py",
            "-        self.assertEqual(a, b)\n-        assert ok\n",
        )
        self.assertEqual(
            self.found(diff),
            {"scripts/test_fold_changelog.py": (["self.assertEqual(a, b)", "assert ok"], 0)},
        )

    def test_a_deleted_test_file_is_listed(self):
        diff = (
            "diff --git a/frontend/src/x.test.ts b/frontend/src/x.test.ts\n"
            "deleted file mode 100644\n"
            "--- a/frontend/src/x.test.ts\n"
            "+++ /dev/null\n"
            "@@ -1 +0,0 @@\n"
            "-expect(1).toBe(1);\n"
        )
        self.assertEqual(self.found(diff), {"frontend/src/x.test.ts": (["expect(1).toBe(1);"], 0)})

    def test_mnemonic_prefixes_and_removed_lines_that_look_like_headers(self):
        diff = file_diff(
            "crates/git-core/tests/a.rs",
            "--- not a header\n-    assert!(gone);\n",
            old="c",
            new="w",
        )
        self.assertEqual(self.found(diff), {"crates/git-core/tests/a.rs": (["assert!(gone);"], 0)})

    def test_report_lists_files_and_lines_after_the_marker(self):
        text = module.report({"crates/a/tests/t.rs": (["assert!(x);"], 2)})
        self.assertTrue(text.startswith(module.MARKER))
        self.assertIn("| `crates/a/tests/t.rs` | 1 | 2 |", text)
        self.assertIn("assert!(x);", text)

    def test_report_without_findings_says_so(self):
        text = module.report({})
        self.assertTrue(text.startswith(module.MARKER))
        self.assertIn("removes or rewrites no assertion", text)


if __name__ == "__main__":
    unittest.main()
