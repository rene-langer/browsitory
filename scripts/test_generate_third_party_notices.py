#!/usr/bin/env python3
"""Tests for generate-third-party-notices.py (no cargo, no pnpm)."""

import importlib.util
import pathlib
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
from unittest import mock

MODULE_PATH = pathlib.Path(__file__).resolve().parent / "generate-third-party-notices.py"
spec = importlib.util.spec_from_file_location("generate_third_party_notices", MODULE_PATH)
gen = importlib.util.module_from_spec(spec)
assert spec.loader is not None
sys.modules[spec.name] = gen
spec.loader.exec_module(gen)


LOCK = """
[[package]]
name = "libgit2-sys"
version = "0.18.8+1.9.7"

[[package]]
name = "libssh2-sys"
version = "0.3.3"

[[package]]
name = "openssl-src"
version = "300.6.1+3.6.3"

[[package]]
name = "libz-sys"
version = "1.1.31"

[[package]]
name = "serde"
version = "1.0.0"
"""

HEADER = """# Third-party notices

- **libgit2** 1.9.7 (through the `libgit2-sys` crate 0.18.8+1.9.7): GPL-2.0.
- **libssh2** (through the `libssh2-sys` crate 0.3.3): BSD-3-Clause.
- **OpenSSL** 3.6.3 (through the `openssl-src` crate 300.6.1+3.6.3), Linux and macOS only.
- **zlib** (through the `libz-sys`
  crate 1.1.31): Zlib.
"""


class NativeVersionTests(unittest.TestCase):
    def test_matching_header_passes(self):
        self.assertEqual(gen.native_version_problems(HEADER, LOCK), [])

    def test_a_bumped_sys_crate_fails(self):
        lock = LOCK.replace('"0.3.3"', '"0.3.4"')
        problems = gen.native_version_problems(HEADER, lock)
        self.assertEqual(len(problems), 1)
        self.assertIn("libssh2-sys ['0.3.3']", problems[0])
        self.assertIn("Cargo.lock has ['0.3.4']", problems[0])

    def test_a_crate_missing_from_the_header_fails(self):
        header = HEADER.replace("the `libz-sys`\n  crate 1.1.31", "zlib")
        problems = gen.native_version_problems(header, LOCK)
        self.assertEqual(len(problems), 1)
        self.assertIn("libz-sys nowhere", problems[0])

    def test_the_real_header_matches_the_real_lockfile(self):
        header = gen.HEADER.read_text(encoding="utf-8")
        lock = (gen.ROOT / "Cargo.lock").read_text(encoding="utf-8")
        self.assertEqual(gen.native_version_problems(header, lock), [])


class GroupingTests(unittest.TestCase):
    def test_identical_texts_share_a_group_and_line_endings_do_not_matter(self):
        groups = gen.group_by_text(
            [
                ("b 1.0.0", "MIT", "Copyright X\r\n\r\nPermission...  \r\n"),
                ("a 2.0.0", "MIT", "\nCopyright X\n\nPermission...\n"),
                ("c 1.0.0", "MIT", "Copyright Y"),
                ("d 1.0.0", "Apache-2.0", "Apache text"),
            ]
        )
        self.assertEqual([(g.license, sorted(g.users)) for g in groups], [
            ("Apache-2.0", ["d 1.0.0"]),
            ("MIT", ["a 2.0.0", "b 1.0.0"]),
            ("MIT", ["c 1.0.0"]),
        ])
        self.assertEqual(groups[1].text, "Copyright X\n\nPermission...")

    def test_fence_outgrows_backticks_in_the_text(self):
        self.assertEqual(gen.fence("plain"), "```")
        self.assertEqual(gen.fence("has ```` four"), "`````")

    def test_render_lists_each_licence_then_the_texts(self):
        groups = gen.group_by_text([("b 1.0.0", "MIT", "T1"), ("a 1.0.0", "MIT", "T1"), ("z 3.0.0", "ISC", "T2")])
        out = gen.render("# Header\r\n", groups, [])
        self.assertTrue(out.startswith("# Header\n\n## Rust crates\n"))
        self.assertIn("- **ISC**: z 3.0.0\n- **MIT**: a 1.0.0, b 1.0.0\n", out)
        self.assertIn("### MIT: a 1.0.0 and 1 more\n\nUsed by: a 1.0.0, b 1.0.0.\n\n```text\nT1\n```", out)
        self.assertIn("## JavaScript packages", out)
        self.assertTrue(out.endswith(gen.FOOTER + "\n"))


class RustEntriesTests(unittest.TestCase):
    ABOUT = {
        "licenses": [
            {
                "id": "MIT",
                "text": "MIT text",
                "used_by": [
                    {"crate": {"name": "serde", "version": "1.0.0"}},
                    {"crate": {"name": "serde_derive", "version": "1.0.0"}},
                ],
            },
            {"id": "MPL-2.0", "text": "MPL text", "used_by": [{"crate": {"name": "option-ext", "version": "0.2.0"}}]},
        ]
    }

    def test_keeps_only_shipped_crates(self):
        shipped = {("serde", "1.0.0"), ("option-ext", "0.2.0")}
        self.assertEqual(
            gen.rust_entries(self.ABOUT, shipped),
            [("serde 1.0.0", "MIT", "MIT text"), ("option-ext 0.2.0", "MPL-2.0", "MPL text")],
        )

    def test_a_shipped_crate_without_a_licence_fails(self):
        with self.assertRaisesRegex(gen.NoticesError, "no licence text for: zlib-rs 0.6.8"):
            gen.rust_entries(self.ABOUT, {("serde", "1.0.0"), ("zlib-rs", "0.6.8")})

    def test_parse_cargo_tree_skips_workspace_crates_and_duplicates(self):
        output = (
            "tauri-app v0.1.0 (/repo/crates/tauri-app)\n"
            "serde v1.0.0\n"
            "serde v1.0.0 (*)\n"
            "windows-sys v0.59.0\n"
            "\n"
        )
        self.assertEqual(
            gen.parse_cargo_tree(output, {"tauri-app"}), {("serde", "1.0.0"), ("windows-sys", "0.59.0")}
        )

    def test_about_targets_reads_the_config(self):
        self.assertIn("x86_64-pc-windows-msvc", gen.about_targets())


class NpmEntriesTests(unittest.TestCase):
    def test_reads_licence_files_and_skips_type_packages(self):
        with tempfile.TemporaryDirectory() as tmp:
            pkg = pathlib.Path(tmp) / "react"
            pkg.mkdir()
            (pkg / "LICENSE").write_text("MIT License\nCopyright Meta")
            (pkg / "index.js").write_text("code")
            bare = pathlib.Path(tmp) / "bare"
            bare.mkdir()
            data = {
                "MIT": [
                    {"name": "react", "versions": ["19.0.0"], "paths": [str(pkg)], "license": "MIT"},
                    {"name": "@types/react", "versions": ["19.0.0"], "paths": [str(pkg)], "license": "MIT"},
                    {"name": "bare", "versions": ["1.0.0"], "paths": [str(bare)], "license": "MIT"},
                ]
            }
            entries, used = gen.npm_entries(data, fallbacks={})
        self.assertEqual(entries[0], ("react 19.0.0", "MIT", "MIT License\nCopyright Meta"))
        self.assertEqual(entries[1][0], "bare 1.0.0")
        self.assertIn("ships no licence file", entries[1][2])
        self.assertEqual(len(entries), 2)
        self.assertEqual(used, set())

    def test_a_package_without_licence_file_uses_its_fallback(self):
        with tempfile.TemporaryDirectory() as tmp:
            data = {"MIT OR Apache-2.0": [{"name": "@tauri-apps/plugin-log", "versions": ["2.0.0"], "paths": [tmp]}]}
            entries, used = gen.npm_entries(data)
        self.assertEqual(used, {"@tauri-apps/plugin-*"})
        self.assertIn("Tauri Apps Contributors", entries[0][2])
        self.assertEqual(entries[0][1], "MIT OR Apache-2.0")

    def test_several_licence_files_are_all_kept(self):
        with tempfile.TemporaryDirectory() as tmp:
            pkg = pathlib.Path(tmp)
            (pkg / "LICENSE-MIT").write_text("mit")
            (pkg / "LICENSE-APACHE").write_text("apache")
            (pkg / "license-checker.js").write_text("not a licence")
            texts = gen.package_license_texts(pkg)
        self.assertEqual([name for name, _ in texts], ["LICENSE-APACHE", "LICENSE-MIT"])


class MainTests(unittest.TestCase):
    def run_main(self, argv, content, current):
        with tempfile.TemporaryDirectory() as tmp:
            out = pathlib.Path(tmp) / "THIRD_PARTY_NOTICES.md"
            if current is not None:
                out.write_bytes(current.encode())
            stdout, stderr = StringIO(), StringIO()
            with mock.patch.object(gen, "OUTPUT", out), redirect_stdout(stdout), redirect_stderr(stderr):
                code = gen.main(argv, generate_fn=lambda: content)
            return code, out.read_text() if out.exists() else None, stderr.getvalue()

    def test_check_passes_when_current_even_with_crlf(self):
        code, _, _ = self.run_main(["--check"], "a\nb\n", "a\r\nb\r\n")
        self.assertEqual(code, 0)

    def test_check_fails_with_the_command_to_run(self):
        code, after, err = self.run_main(["--check"], "new\n", "old\n")
        self.assertEqual(code, 1)
        self.assertEqual(after, "old\n")
        self.assertIn(gen.REGENERATE, err)

    def test_generate_writes_the_file(self):
        code, after, _ = self.run_main([], "new\n", None)
        self.assertEqual((code, after), (0, "new\n"))

    def test_generation_errors_fail(self):
        def broken():
            raise gen.NoticesError("Cargo.lock has 0.3.4")

        with redirect_stderr(StringIO()) as err:
            self.assertEqual(gen.main(["--check"], generate_fn=broken), 1)
        self.assertIn("Cargo.lock has 0.3.4", err.getvalue())


if __name__ == "__main__":
    unittest.main()
