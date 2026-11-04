### Stash apply checks case-only names where names fold

- Fixed: Apply and Pop now refuse a stash whose untracked file `d/x` needs a folder where a
  tracked file differs only in case (`D`). This applies when the repository sets
  `core.ignorecase`, and always on macOS and Windows, even if a copied repository says
  otherwise. On a case-insensitive drive `d` and `D` are the same name, so the stash could not be
  applied cleanly, and git refuses it too. On a case-sensitive drive with `core.ignorecase` set,
  git would apply it; Browsitory refuses on purpose, to be safe. Nothing is written and the
  stash is kept. (#234)
