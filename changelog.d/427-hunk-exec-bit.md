### Executable files and hunks

- Fixed: staging, unstaging or discarding a hunk or selected lines of an executable file dropped
  its executable bit, in the index (the next commit lost it) or in the work tree, on Linux and
  macOS. As with `git add -p` and `git checkout -p`, the file now keeps its mode, and a pending
  exec bit change is neither staged nor reverted by a hunk or line operation (#427).
