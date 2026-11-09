### Files that differ only in case

- Fixed: with `core.ignorecase` on in a submodule (or set globally for a new submodule clone),
  updating the submodule to a commit whose files include two names that differ only in case
  (`A` and `a`) that its current commit doesn't kept only one of the two in the submodule's
  index and didn't write the other's file. The update is now refused before anything is
  written and names both files (#534).
