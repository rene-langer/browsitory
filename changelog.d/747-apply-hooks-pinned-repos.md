### Patches and the repository picker

- Added: "Apply as commits" runs the `applypatch-msg`, `pre-applypatch` and `post-applypatch`
  hooks like `git am`; a rejecting hook takes the whole patch file back and shows its output
  (#747).
- Added: recent repositories can be pinned in the picker: pinned ones come first and don't count
  against the ten recent ones (#766).
