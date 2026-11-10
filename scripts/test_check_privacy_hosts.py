#!/usr/bin/env python3
"""Unit tests for check-privacy-hosts.py (#627). Run: python3 scripts/test_check_privacy_hosts.py"""

import importlib.util
import tempfile
import unittest
from unittest import mock
from pathlib import Path

_spec = importlib.util.spec_from_file_location(
    "check_privacy_hosts", Path(__file__).with_name("check-privacy-hosts.py")
)
cph = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cph)

NOTICE = "".join(f"`{host}`\n" for host in cph.ALLOWED_HOSTS)
CONF = '{"endpoints": ["https://github.com/x", "https://rene-langer.github.io/x"]}'
RUST = 'const A: &str = "https://api.github.com";\nconst B: &str = "https://api.bitbucket.org/2.0";\n'


class CheckPrivacyHostsTest(unittest.TestCase):
    def setUp(self):
        # The real tree's dynamic-URL lines aren't in these synthetic trees.
        patcher = mock.patch.dict(cph.DYNAMIC_URLS, clear=True)
        patcher.start()
        self.addCleanup(patcher.stop)

    def repo(self, files: dict[str, str]) -> Path:
        root = Path(tempfile.mkdtemp())
        base = {
            "docs/guide/privacy.md": NOTICE,
            "crates/tauri-app/tauri.conf.json": CONF,
            "crates/repo-service/src/forge.rs": RUST,
        }
        for name, text in {**base, **files}.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        return root

    def test_passes_when_every_host_is_allowed_and_documented(self):
        self.assertEqual(cph.check(self.repo({})), [])

    def test_fails_on_a_new_host_in_frontend_code(self):
        errors = cph.check(self.repo({"frontend/src/a.ts": 'fetch("https://stats.example.net.io/x");\n'}))
        self.assertEqual(len(errors), 1)
        self.assertIn("stats.example.net.io", errors[0])

    def test_fails_on_a_new_host_in_rust_code_after_a_test_module(self):
        text = (
            "#[cfg(test)]\nmod tests {\n    const T: &str = \"https://in-test.io\";\n}\n"
            'const U: &str = "https://after.io";\n'
        )
        errors = cph.check(self.repo({"crates/x/src/lib.rs": text}))
        self.assertEqual(len(errors), 1)
        self.assertIn("after.io", errors[0])

    def test_ignores_tests_fixtures_and_example_hosts(self):
        files = {
            "frontend/src/a.test.ts": 'fetch("https://test-only.io");\n',
            "extension/src/fixtures/f.ts": 'fetch("https://fixture.io");\n',
            "crates/x/src/tests.rs": 'const A: &str = "https://tests-rs.io";\n',
            "crates/test-support/src/lib.rs": 'const A: &str = "https://support.io";\n',
            "crates/x/src/lib.rs": 'const A: &str = "https://git.example.test/r.git http://127.0.0.1:9 https://example.com";\n',
            "frontend/public/icon.svg": '<svg xmlns="http://www.w3.org/2000/svg"/>\n',
        }
        self.assertEqual(cph.check(self.repo(files)), [])

    def test_ignores_the_files_of_test_only_rust_modules(self):
        files = {
            "crates/x/src/lib.rs": "#[cfg(test)]\nmod snapshots;\n",
            "crates/x/src/snapshots.rs": 'const A: &str = "https://snap.io";\n',
            "crates/x/src/cmd.rs": "#[cfg(test)]\nmod inner;\n",
            "crates/x/src/cmd/inner/mod.rs": 'const A: &str = "https://inner.io";\n',
        }
        self.assertEqual(cph.check(self.repo(files)), [])

    def assert_flags(self, host: str, files: dict[str, str]):
        errors = cph.check(self.repo(files))
        self.assertEqual(len(errors), 1, errors)
        self.assertIn(host, errors[0])

    def test_scans_comments(self):
        for text in ("// see https://docs.io\n", "/* https://block.io\n */\n",
                     "const x = 1; // https://docs.io\n"):
            with self.subTest(text=text):
                self.assert_flags(".io", {"frontend/src/a.ts": text})

    def test_finds_a_url_after_a_double_slash_inside_a_string(self):
        self.assert_flags("new-host.net", {
            "crates/x/src/lib.rs": 'let s = "a//b"; let u = "https://new-host.net";\n'})

    def test_finds_markup_after_an_html_comment(self):
        self.assert_flags("cdn.io", {
            "frontend/index.html": '<!-- fonts --><script src="https://cdn.io/x.js"></script>\n'})

    def test_matches_the_scheme_in_any_case(self):
        self.assert_flags("upper.io", {"frontend/src/a.ts": 'fetch("HTTPS://Upper.io/x");\n'})

    def test_finds_protocol_relative_urls(self):
        self.assert_flags("cdn.host", {"frontend/src/a.ts": 'load("//cdn.host/x.js");\n'})
        self.assert_flags("fonts.host", {"frontend/src/a.css": "a { b: url(//fonts.host/f.woff2); }\n"})

    def test_flags_urls_without_a_literal_host(self):
        for text in ('format!("https://{HOST}/x")\n', 'concat!("https://", "host.io")\n',
                     "fetch(`https://${host}/x`);\n", 'fetch("https://" + host);\n'):
            with self.subTest(text=text):
                self.assert_flags("without a literal host", {"crates/x/src/lib.rs": text})

    def test_accepts_dynamic_urls_on_the_list_and_flags_stale_entries(self):
        cph.DYNAMIC_URLS["crates/x/src/lib.rs"] = {'format!("https://{host}/x")': "user host"}
        self.assertEqual(cph.check(self.repo({"crates/x/src/lib.rs": 'format!("https://{host}/x")\n'})), [])
        self.assert_flags("matches no line", {"crates/x/src/lib.rs": "fn main() {}\n"})

    def test_scans_html_public_vite_and_json_files(self):
        for name in ("frontend/index.html", "frontend/public/manifest.webmanifest",
                     "frontend/vite.config.ts", "frontend/src/data.json", "extension/package.json",
                     "crates/tauri-app/capabilities/default.json"):
            with self.subTest(name=name):
                self.assert_flags("cdn.io", {name: '{"u": "https://cdn.io/x"}\n'})

    def test_fails_when_the_notice_does_not_name_an_allowed_host(self):
        root = self.repo({"docs/guide/privacy.md": NOTICE.replace("`api.github.com`", "")})
        errors = cph.check(root)
        self.assertEqual(len(errors), 1)
        self.assertIn("doesn't name `api.github.com`", errors[0])

    def test_fails_when_an_allowed_host_is_no_longer_used(self):
        root = self.repo({"crates/repo-service/src/forge.rs": 'const A: &str = "https://api.github.com";\n'})
        errors = cph.check(root)
        self.assertEqual(len(errors), 1)
        self.assertIn("api.bitbucket.org", errors[0])


if __name__ == "__main__":
    unittest.main()
