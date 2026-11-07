#!/usr/bin/env python3
"""Tests for check-docs-nav.py, on throwaway sites in a temp directory."""

import importlib.util
import pathlib
import tempfile
import unittest

MODULE_PATH = pathlib.Path(__file__).resolve().parent / "check-docs-nav.py"
spec = importlib.util.spec_from_file_location("check_docs_nav", MODULE_PATH)
check_docs_nav = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(check_docs_nav)

CONFIG = """\
site_name: Test
docs_dir: docs/guide
nav:
  - Home: README.md
  - Section:
      - a.md
# a comment after the nav
strict: true
"""


class CheckDocsNavTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.tmp.name)
        self.docs = self.root / "docs" / "guide"
        (self.docs / "assets").mkdir(parents=True)
        (self.docs / "assets" / "shot.png").write_bytes(b"")
        (self.root / "mkdocs.yml").write_text(CONFIG)
        self.write("README.md", "# Home\n\nSee [a](a.md) and ![shot](assets/shot.png).\n")
        self.write("a.md", "# A\n\n[Home](README.md#home), [web](https://example.com).\n")

    def tearDown(self):
        self.tmp.cleanup()

    def write(self, name, text):
        (self.docs / name).write_text(text)

    def problems(self):
        return check_docs_nav.check(self.root)

    def test_clean_site_passes(self):
        self.assertEqual(self.problems(), [])

    def test_nav_entry_without_page_fails(self):
        (self.root / "mkdocs.yml").write_text(CONFIG.replace("      - a.md\n", "      - a.md\n      - gone.md\n"))
        self.assertEqual(self.problems(), ["mkdocs.yml: nav entry gone.md does not exist in docs/guide/"])

    def test_page_missing_from_nav_fails(self):
        self.write("orphan.md", "# Orphan\n")
        self.assertEqual(self.problems(), ["docs/guide/orphan.md: page is not in mkdocs.yml's nav"])

    def test_absolute_links_fail(self):
        self.write("a.md", '# A\n\n[x](/install/)\n\n[r]: /tags/\n\n<img src="/assets/shot.png">\n')
        self.assertEqual(
            self.problems(),
            [
                "docs/guide/a.md:3: absolute link /install/; use a relative link",
                "docs/guide/a.md:5: absolute link /tags/; use a relative link",
                "docs/guide/a.md:7: absolute link /assets/shot.png; use a relative link",
            ],
        )

    def test_missing_image_fails(self):
        self.write("a.md", "# A\n\n![gone](assets/gone.png)\n")
        self.assertEqual(self.problems(), ["docs/guide/a.md:3: link target assets/gone.png does not exist"])

    def test_links_in_code_are_ignored(self):
        self.write("a.md", "# A\n\n`[x](/abs/)`\n\n```\n[y](/abs/)\n```\n")
        self.assertEqual(self.problems(), [])

    def test_unparseable_nav_is_an_error(self):
        (self.root / "mkdocs.yml").write_text(CONFIG.replace("  - Home: README.md", "  - {Home: README.md}"))
        with self.assertRaises(ValueError):
            self.problems()


if __name__ == "__main__":
    unittest.main()
