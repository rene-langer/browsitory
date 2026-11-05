### Mark resolved on a folder

- Fixed: when a conflicted path had become a folder, Mark resolved opened it again by its path
  before staging it. Something swapped in at that moment could hang Browsitory (a FIFO) or be
  staged in its place (a link, or another repository). Browsitory now holds the folder open and
  reads the nested repository's commit through it, never by path. A FIFO or link inside the
  nested repository is not opened or followed either, and its files are read only up to a size
  limit (#213).
- Fixed: Mark resolved on a file below a folder that has become a file (for example `a/b/c.txt`
  where `a` is now a file) failed with "Not a directory". It now stages the deletion, as
  `git add` does. Discard hunk and Discard lines there now say which file is in the way, as
  Discard does, instead of a raw error (#213).
- Changed: a nested repository is still staged as its checked-out commit, as `git add` does,
  also when it is a submodule (its git folder in `.git/modules`, nested ones included) or a
  linked worktree of a repository in the work tree. Browsitory never opens anything outside the
  work tree for this. A repository whose git folder is elsewhere, or at a path Browsitory can't
  match to the work tree, is refused; stage it with `git add <path>` in a terminal. That
  includes a linked worktree of a repository outside the work tree, and one git wrote with a
  path the work tree wasn't opened by (for example macOS `/private/var` for `/var`) (#213).
- Changed: when Browsitory can't stage the folder, it now says why instead of showing a raw
  git error, and how to stage it with git: the folder isn't a repository, the repository has no
  commit yet, its git folder is outside the work tree or can't be matched to it, a linked
  worktree's main repository can't be found, or Browsitory can't read it (a SHA-256 or reftable
  repository, a ref name git refuses, or a ref or `packed-refs` that isn't a regular file)
  (#213).
