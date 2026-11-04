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

## Checks

`scripts/check-changelog.py` (pre-push hook and CI) accepts a change to `crates/`,
`frontend/src/` or `e2e/` when it adds or edits a fragment here (or edits `CHANGELOG.md`). It
also checks that each added or edited fragment starts with a `### ` heading and has entries.
This README is never a fragment.

## Releasing

Before a release, `scripts/fold-changelog.py` moves all fragments to the top of
`## [Unreleased]` in `CHANGELOG.md`, newest first (by the date of the commit that added each
one; ties and uncommitted files by file name), and deletes them. The release workflow fails
while fragments are left. See `docs/RELEASING.md`.
