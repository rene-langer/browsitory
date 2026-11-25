#!/usr/bin/env python3
"""Check that a release tag may be released (release.yml's `verify` job, #1111)."""


class CheckError(Exception):
    pass


def parse_tag(tag):
    raise NotImplementedError


def changelog_problems(text, version):
    raise NotImplementedError


def ci_problems(runs):
    raise NotImplementedError


def is_on_main(root, commit, main_ref):
    raise NotImplementedError


def main(argv=None, list_ci_runs=None):
    raise NotImplementedError
