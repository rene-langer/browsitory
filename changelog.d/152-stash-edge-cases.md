### Stash edge cases

- Fixed: "Stash file…" on a staged rename stashes the whole rename. Before, it stashed only the new
  name and left the old name's deletion staged. If a new file was made at the old name since,
  it refuses instead of stashing that file too. Where names fold (macOS, Windows,
  `core.ignorecase`), "Stash file…" refuses a rename that only changes case, since the stash
  couldn't undo it (#152).
- Fixed: "Stash file…" on a file that was removed from the index, ignored and then edited now
  refuses with the same clear message as "Stash all", like git. Before, the stash put the
  committed version back and tracked the file again (#259).
- Fixed: stashing with a repository staged inside this one (`git add lib`, where `lib` has its
  own `.git`) showed a raw libgit2 error. "Stash all", and "Stash file…" on it, now refuse with a
  clear message. A new submodule that isn't committed yet, or a staged link whose folder is gone,
  gets its own message (#244).
