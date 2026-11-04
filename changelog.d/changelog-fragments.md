### Changelog fragments (internal)

Pull requests no longer conflict over `CHANGELOG.md`. Each one now adds its entry as a file in
`changelog.d/`, and a release folds them into `CHANGELOG.md`.

- Changed: `scripts/check-changelog.py` (pre-push hook and CI) accepts a new or edited fragment,
  `changelog.d/<issue>-<short-slug>.md`, as the changelog entry, and checks that it starts with
  a `### ` heading. Editing `CHANGELOG.md` still counts. The format is in `changelog.d/README.md`.
- Added: `scripts/fold-changelog.py` moves all fragments to the top of `## [Unreleased]`,
  newest first, and deletes them. With `--check` it fails while fragments are left; the release
  workflow runs that check, so a release always ships a complete `CHANGELOG.md`.
