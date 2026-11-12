### Faster file and line history

- Changed: file history and line history read only the folders on the file's path that changed
  between a commit and its parent, instead of looking the whole path up twice per commit. On a
  repository with 130,000 commits, the history of a rarely changed file takes about 4 s instead
  of a minute (#838).
