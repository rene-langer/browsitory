### Mark resolved on a folder

- Fixed: when a conflicted path had become a folder, Mark resolved opened it again by its path
  before staging it. Something swapped in at that moment could hang Browsitory (a FIFO) or be
  staged in its place (a link, or another repository). Browsitory now holds the folder open and
  reads the nested repository's commit through it, never by path. A FIFO or link inside the
  nested repository is not opened or followed either, and its files are read only up to a size
  limit (#213).
- Fixed: Mark resolved on a file below a folder that has become a file (for example `a/b/c.txt`
  where `a` is now a file) failed with "Not a directory". It now stages the deletion, as
  `git add` does (#213).
- Changed: a nested repository is still staged as its checked-out commit, as `git add` does,
  also when its `.git` is a `gitdir:` file (a submodule) or it is a linked worktree. When
  Browsitory can't stage the folder, it now says why instead of showing a raw git error: the
  folder is not a repository (also when its `.git` is a link or its git folder is gone), the
  repository has no commit yet, or Browsitory can't read it. Stage such a folder with git. It
  can't read a SHA-256 or reftable repository, a ref that is a link, or a git folder given as a
  relative path out of the work tree. On Windows, it never opens a git folder on a network
  share (#213).
