### Mark resolved on a folder

- Fixed: when a conflicted path had become a folder, Mark resolved opened it again by its path
  before staging it. Something swapped in at that moment could hang Browsitory (a FIFO) or be
  staged in its place (a link). Browsitory now holds the folder open and reads the nested
  repository's commit through it, never by path. A FIFO or link inside the nested repository
  is not opened or followed either (#213).
- Changed: a nested repository is still staged as its checked-out commit, as `git add` does,
  also when its `.git` is a `gitdir:` file, as a submodule's is. A folder that isn't a
  repository with a commit checked out, or whose `.git` is a link, is refused with "'…' is a
  folder. Only a repository with a commit checked out can be marked resolved as a folder."
  Before, it failed with a raw git error, or a `.git` link was followed (#213).
