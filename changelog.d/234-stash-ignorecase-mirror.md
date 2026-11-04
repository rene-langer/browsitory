### Stash apply checks case-only names under core.ignorecase

- Fixed: with `core.ignorecase` on, Apply and Pop now refuse a stash whose untracked file `d/x`
  needs a folder where a tracked file differs only in case (`D`). On a case-insensitive drive
  `d` and `D` are the same name, so the stash could not be applied cleanly; git refuses it too.
  Nothing is written and the stash is kept. (#234)
