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


class RunTests(unittest.TestCase):
    def test_decodes_utf8(self):
        seen = {}

        class Done:
            stdout = "ok"

        def run(cmd, **kwargs):
            seen.update(kwargs)
            return Done()

        self.assertEqual(gen._run(["cargo", "about"], run=run), "ok")
        self.assertEqual(seen["encoding"], "utf-8")


class GrammarTests(unittest.TestCase):
    MODULES = {
        "ruby": "import cpp from './cpp.mjs'\nimport yaml from './yaml.mjs'\nexport { default as x } from './xml.mjs'\nconst lang = 1",
        "xml": "",
        "cpp": "import glsl from './glsl.mjs'\n",
        "glsl": "",
        "yaml": "",
        "rust": "",
    }
    HIGHLIGHT = (
        'const LOADERS: Record<HighlightLanguage, () => Promise<unknown>> = {\n'
        '  ruby: () => import("shiki/langs/ruby.mjs"),\n'
        '  rust: () => import("shiki/langs/rust.mjs"),\n'
        "};\n"
    )

    def test_shipped_set_follows_imports_and_stops_at_excluded_grammars(self):
        shipped = gen.shipped_grammars(self.HIGHLIGHT, self.MODULES.__getitem__, {"cpp", "glsl"})
        self.assertEqual(shipped, {"ruby", "yaml", "xml", "rust"})
        self.assertIn("glsl", gen.shipped_grammars(self.HIGHLIGHT, self.MODULES.__getitem__, set()))

    def test_a_loader_the_walk_cannot_read_fails(self):
        highlight = self.HIGHLIGHT.replace("};", '  go: () => import(`shiki/langs/go.mjs`),\n};')
        with self.assertRaisesRegex(gen.NoticesError, "3 LOADERS entries but 2"):
            gen.shipped_grammars(highlight, self.MODULES.__getitem__, set())

    def test_the_real_highlight_loaders_are_all_read(self):
        source = gen.HIGHLIGHT_TS.read_text(encoding="utf-8")
        modules = {name: "" for name in gen.loader_roots(source)}
        self.assertEqual(gen.shipped_grammars(source, modules.__getitem__, set()), set(modules))

    def test_excluded_grammars_reads_the_typescript_list(self):
        source = gen.EXCLUDED_GRAMMARS_TS.read_text(encoding="utf-8")
        self.assertEqual(gen.excluded_grammars(source), {"cpp", "cpp-macro", "glsl"})

    def test_parse_notice_maps_each_file_to_its_section(self):
        notice = (
            "THIRD-PARTY SOFTWARE NOTICES\n"
            + "=" * 105 + "\nFiles:   c.json, rust.json\nLicense: https://x\nSPDX:    MIT\n"
            + "-" * 105 + "\nThe MIT License (MIT)\n=====================\n\nCopyright (c) X\n\nPermission text\n"
            + "=" * 105 + "\nFiles:   ada.json\nLicense: https://y\nSPDX:    GPL-3.0\n" + "-" * 105 + "\nGPL text\n"
        )
        sections = gen.parse_grammar_notice(notice)
        # haml's real NOTICE text underlines its title with "=" lines; that is not a separator.
        self.assertEqual(
            sections["rust"],
            ("MIT", "The MIT License (MIT)\n=====================\n\nCopyright (c) X\n\nPermission text\n"),
        )
        self.assertEqual(sections["c"], sections["rust"])
        self.assertEqual(sections["ada"][0], "GPL-3.0")

    def entries(self, shipped, metadata, exceptions=None):
        notice = {"rust": ("MIT", "MIT text"), "glsl": ("", "")}
        exceptions = {"yaml": gen.GrammarException("TextMate", "t.txt", "r")} if exceptions is None else exceptions
        return gen.grammar_entries(
            shipped, metadata, notice, frozenset({"MIT"}), read_exception_text=lambda f: f"text of {f}", exceptions=exceptions
        )

    def test_allowed_and_excepted_grammars_pass(self):
        entries, problems = self.entries({"rust", "yaml"}, {"rust": {"license": "MIT"}, "yaml": {}})
        self.assertEqual(problems, [])
        self.assertEqual(entries, [("rust", "MIT", "MIT text"), ("yaml", "TextMate", "text of t.txt")])

    def test_an_unlicensed_or_unknown_grammar_fails(self):
        _, problems = self.entries({"glsl", "mystery", "yaml"}, {"glsl": {"source": "https://g"}})
        self.assertTrue(any("grammar glsl (https://g) is licensed nowhere" in p for p in problems), problems)
        self.assertTrue(any("mystery has no tm-grammars metadata" in p for p in problems), problems)

    def test_an_empty_licence_text_fails(self):
        _, problems = gen.grammar_entries(
            {"rust"}, {"rust": {"license": "MIT"}}, {"rust": ("MIT", " \n ")}, frozenset({"MIT"}), exceptions={}
        )
        self.assertEqual(problems, ["grammar rust has an empty licence text in tm-grammars' NOTICE"])

    @unittest.skipUnless(
        (gen.FRONTEND / "node_modules" / "tm-grammars" / "NOTICE").is_file(),
        "needs `pnpm install` in frontend/ (CI installs it before running these tests)",
    )
    def test_the_real_notice_gives_every_shipped_grammar_a_complete_text(self):
        notice = gen.parse_grammar_notice((gen.FRONTEND / "node_modules" / "tm-grammars" / "NOTICE").read_text(encoding="utf-8"))
        haml = notice["haml"][1]
        self.assertIn("Copyright", haml)
        self.assertIn("Permission is hereby granted", haml)

    def test_an_unused_exception_fails(self):
        _, problems = self.entries({"rust"}, {"rust": {"license": "MIT"}})
        self.assertEqual(problems, ["GRAMMAR_EXCEPTIONS entry yaml matches no shipped grammar; remove it"])

    def test_drift_ignores_the_keys_shiki_adds(self):
        upstream = {"name": "x", "patterns": [1]}
        self.assertEqual(gen.grammar_drift({**upstream, "aliases": ["y"], "embeddedLangs": ["c"]}, upstream), [])
        self.assertEqual(gen.grammar_drift({"name": "x", "patterns": [2]}, upstream), ["patterns"])

    def test_shiki_grammar_reads_the_embedded_json(self):
        module = 'const lang = Object.freeze(JSON.parse("{\\"name\\":\\"x\\"}"))'
        self.assertEqual(gen.shiki_grammar(module), {"name": "x"})


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
