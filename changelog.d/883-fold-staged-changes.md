### Fold staged changes into a commit

- Added: with changes staged, the commit menu (and the palette) offers **Fold staged changes
  into this commit**: like `git commit --fixup` and `git rebase --autosquash` in one step, the
  commit keeps its message and author and the commits after it are replayed (the latest commit
  is amended). A conflict pauses it like any rebase; Abort puts the branch back with the changes
  staged again. One Undo step ("Fold staged changes into 1a2b3c4") (#883).
