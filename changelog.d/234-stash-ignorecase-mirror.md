### Stash apply checks case-only names on case-insensitive drives

- Fixed: Apply and Pop now refuse a stash whose untracked file `d/x` needs a folder where a
  tracked file differs only in case (`D`). This applies with `core.ignorecase` on, and always on
  macOS and Windows, even if a copied repository says otherwise. On such a drive `d` and `D` are
  the same name, so the stash could not be applied cleanly; git refuses it too. Nothing is
  written and the stash is kept. (#234)
