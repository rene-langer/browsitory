# Changelog fragments

Every pull request adds its changelog entry as a new file in this folder instead of editing
`CHANGELOG.md`. Two open pull requests then never conflict over the changelog.

## File name

`changelog.d/<issue>-<short-slug>.md`, for example `240-leftover-merge-state.md`. If the pull
request covers several issues, use the lowest issue number. Without an issue, use a short
descriptive slug alone, for example `changelog-fragments.md`.

## Content

Write exactly the section you would have added to `## [Unreleased]` in `CHANGELOG.md`:

- a `### Heading` line first;
- an optional intro paragraph;
- bullets that start with `Fixed:`, `Added:`, `Changed:`, `Removed:` (or similar), with issue
  references such as `(#240)`.

Wrap lines at about 100 characters. Don't add a `## [Unreleased]` header or any other `#` or
`##` heading.

```markdown
### Leftover merge state

- Fixed: "Amend last commit" was hidden while such a leftover was there (#240).
```

## In-app release notes

The app builds its release notes from the released sections of `CHANGELOG.md`
(`frontend/scripts/generateReleaseNotes.mjs`). Each `### Heading` becomes a group, and a
`Fixed:`, `Added:`, `Changed:`, `Removed:`, `Deprecated:` or `Security:` prefix becomes a label.
Changes only developers care about are left out of the app:

- a section headed exactly `### Internal`, or starting with `### Internal:`;
- a bullet starting with `Internal:`.

A heading that only starts with the word, such as `### Internal links`, is still shown.

## Checks

`scripts/check-changelog.py` (pre-push hook and CI) accepts a change to `crates/`,
`frontend/src/` or `e2e/` when it adds or edits a fragment here (or edits `CHANGELOG.md`). It
also checks that each added or edited fragment starts with a `### ` heading and has at least
one line under it. This README is never a fragment. Any other file here that isn't a `*.md`
fragment (for example `240-x.MD` or `240-x.txt`) fails the check, so a misnamed entry is never
lost.

## Releasing

Before a release, `scripts/fold-changelog.py` moves all fragments to the top of
`## [Unreleased]` in `CHANGELOG.md` and deletes them. Uncommitted fragments go first, by file
name. Committed ones follow, newest first by the date of the commit that added each; ties go by
file name. The release workflow fails while fragments are left. See `docs/RELEASING.md`.
