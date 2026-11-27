#!/usr/bin/env python3
"""Unit tests for check-license-compliance.py. Run: python3 scripts/test_check_license_compliance.py"""

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest import mock

_spec = importlib.util.spec_from_file_location(
    "check_license_compliance", Path(__file__).with_name("check-license-compliance.py")
)
clc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(clc)


class RustDepsTest(unittest.TestCase):
    def rust_deps(self, manifests: dict[str, str]) -> set[str]:
        """`rust_deps()` over a tree holding only these `crates/<name>/Cargo.toml` files."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for crate, text in manifests.items():
                path = root / "crates" / crate / "Cargo.toml"
                path.parent.mkdir(parents=True)
                path.write_text(text, encoding="utf-8")
            with mock.patch.object(clc, "ROOT", root):
                return clc.rust_deps()

    def test_reads_the_top_level_tables(self):
        deps = self.rust_deps(
            {
                "one": '[package]\nname = "one"\n\n'
                '[dependencies]\nserde = "1"\n\n'
                '[dev-dependencies]\ntempfile = "3"\n\n'
                '[build-dependencies]\ncc = "1"\n'
            }
        )
        self.assertEqual(deps, {"serde", "tempfile", "cc"})

    def test_reads_the_platform_specific_tables(self):
        # #1114: a dependency declared for one platform is a direct dependency too.
        deps = self.rust_deps(
            {
                "one": '[package]\nname = "one"\n\n'
                "[target.'cfg(windows)'.dependencies]\nsome-undocumented-crate = \"1\"\n\n"
                "[target.'cfg(unix)'.dev-dependencies]\nanother-undocumented-crate = \"2\"\n\n"
                '[target.x86_64-pc-windows-msvc.build-dependencies]\nwinres = "0.1"\n'
            }
        )
        self.assertEqual(deps, {"some-undocumented-crate", "another-undocumented-crate", "winres"})

    def test_leaves_out_workspace_crates_in_every_table(self):
        deps = self.rust_deps(
            {
                "one": '[package]\nname = "one"\n\n'
                '[dependencies]\ngit-core = { path = "../git-core" }\n\n'
                "[target.'cfg(unix)'.dev-dependencies]\n"
                'test-support = { path = "../test-support" }\nlibc = "0.2"\n'
            }
        )
        self.assertEqual(deps, {"libc"})

    def test_the_real_tree_includes_git_cores_unix_only_libc(self):
        self.assertIn("libc", clc.rust_deps())


if __name__ == "__main__":
    unittest.main()
