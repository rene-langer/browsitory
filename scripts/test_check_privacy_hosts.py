#!/usr/bin/env python3
"""Unit tests for check-privacy-hosts.py (#627). Run: python3 scripts/test_check_privacy_hosts.py"""

import importlib.util
import tempfile
import unittest
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

    def test_ignores_comments_tests_fixtures_and_example_hosts(self):
        files = {
            "frontend/src/a.ts": "// see https://docs.io\n/* https://block.io\n https://b.io */\nconst x = 1; // https://t.io\n",
            "frontend/src/a.test.ts": 'fetch("https://test-only.io");\n',
            "extension/src/fixtures/f.ts": 'fetch("https://fixture.io");\n',
            "crates/x/src/tests.rs": 'const A: &str = "https://tests-rs.io";\n',
            "crates/test-support/src/lib.rs": 'const A: &str = "https://support.io";\n',
            "crates/x/src/lib.rs": 'const A: &str = "https://git.example.test/r.git http://127.0.0.1:9 https://example.com";\n',
        }
        self.assertEqual(cph.check(self.repo(files)), [])

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
