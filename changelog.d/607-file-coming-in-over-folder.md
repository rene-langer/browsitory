### File coming in over a folder

- Fixed: a revert, cherry-pick, merge, rebase step or stash apply that brings a file `c` back
  where your branch now has a folder `c/` (with a name such as `c.txt` sorting between them)
  stopped with "could not open 'c' for writing: Is a directory" and couldn't be done in the app.
  Like git, it now keeps the folder's files and moves the incoming file aside as the conflict
  `c~<label>` (`c~parent of <commit> (<subject>)` for a revert, `c~<commit> (<subject>)` for a
  cherry-pick or rebase step, `c~<branch>`, `c~Stashed changes`), which you resolve or abort.
  Without a name between them the file now gets git's name too (#607).
