### Stash edge cases

- Fixed: "Stash file…" on a staged rename stashes the whole rename. Before, it stashed only the new
  name and left the old name's deletion staged (#152).
- Fixed: "Stash file…" on a file that was removed from the index, ignored and then edited now
  refuses with the same clear message as "Stash all", like git. Before, the stash put the
  committed version back and tracked the file again (#259).
