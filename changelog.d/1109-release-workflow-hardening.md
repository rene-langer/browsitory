### Internal

- Changed: the release workflow pins every action by commit SHA (Renovate updates them), gives
  `contents: write` only to the two jobs that upload to the draft release, and leaves no token in
  `.git/config` after a checkout; CI's `mutants` and `removed-assertions` jobs don't either
  (#1109).
- Changed: the Open VSX publish and dry-run jobs run `ovsx` from `extension/publish/`, a locked
  pnpm project installed without install scripts before the token is used, instead of an
  unlocked `npx ovsx@1.3.0`; Renovate and the licence check now cover it (#1110).
- Added: the release workflow's `verify` job fails unless `CHANGELOG.md` has a section for the
  tag's version right under an empty `## [Unreleased]`, the tagged commit is on `main`, and CI
  passed for it (`scripts/check-release-tag.py`) (#1111).
