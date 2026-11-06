### Worktree names on Windows

- Fixed: on Windows, a worktree whose name or folder ended in a dot or a space was created but
  couldn't be found, removed or rolled back from the app. Such names, and names Windows reserves
  for devices (`con`, `aux`, `nul`, `com1` and the like), are now refused before anything is
  created, with a message naming the part to change (#261).
