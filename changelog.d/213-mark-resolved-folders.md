### Mark resolved on a folder

- Fixed: when a conflicted path had become a folder, Mark resolved opened it again by its path
  before staging it. Something swapped in at that moment could hang Browsitory (a FIFO) or be
  staged in its place (a link, or another repository). Browsitory now holds the folder open and
  reads the nested repository's commit through it, never by path. A FIFO or link inside the
  nested repository is not opened or followed either, and its files are read only up to a size
  limit (#213).
- Changed: a nested repository is still staged as its checked-out commit, as `git add` does,
  also when its `.git` is a `gitdir:` file (a submodule) or it is a linked worktree. When
  Browsitory can't stage the folder, it now says why instead of showing a raw git error: the
  folder is not a repository (also when its `.git` is a link or its git folder is gone), the
  repository has no commit yet, or Browsitory can't read its format (SHA-256, reftable). Stage
  such a folder with git (#213).
