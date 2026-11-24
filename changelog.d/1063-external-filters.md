### Git LFS, git-crypt and other filter drivers

- Fixed: Browsitory can't run filter drivers, but staged and checked out their files anyway: with
  git-crypt unlocked, Stage and Commit committed the secret in plain text, and with Git LFS a
  switch, reset or checkout wrote the pointer text over the real file. Every action that would
  store or write a file whose `filter` attribute names a configured driver (staging, discarding,
  stashing, applying a stash or patch, resolving conflicts, switching, checking out, resetting,
  merging, pulling, cherry-picking, reverting, rebasing, undo) now refuses before it changes
  anything, naming the file and the filter (#1063).
