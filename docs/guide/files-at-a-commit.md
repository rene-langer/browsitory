# Files at a commit

A commit's details list the files it changed. Switch **Changes** to **All files** above that
list to browse every file the commit has, as it was then, without checking the commit out. The
choice stays on while you move to other commits; **Changes** goes back to the diffs.

## Browsing the tree

Folders open when you click them; each loads only then, so even a large tree opens quickly.
Folders come first, then files, each with its size. Type in **Filter files** to search every
path in the commit (case doesn't matter): the tree is replaced by the matching files with their
full paths. At most 500 matches are listed; type more to narrow them down.

The clock button at the end of a folder's row opens its
[folder history](file-history.md#folder-history).

## Viewing a file

Click a file to see its content at that commit, read-only, with line numbers and the same syntax
highlighting as the diffs (the **Syntax highlighting** toggle in a diff's toolbar turns it off
for both). A PNG, JPEG, GIF or WebP image is shown as an image. Binary files, files over 2 MB and
submodules are not shown; a notice says so. A symbolic link shows its target.

The file's header has these actions:

- **History** opens the [file history](file-history.md) of that path.
- **History of lines N–M** (or **Line history** while no lines are selected): select lines of a
  text file with the mouse, then click it for the commits that changed those lines (see
  [History of lines](file-history.md#history-of-lines)).
- **Folder history** lists the commits that changed anything in the file's folder.
- **Compare with working tree** shows the diff between this version and the file in your working
  tree, as **Diff against working tree** does for a whole commit (see
  [Comparing commits and branches](compare.md)). Click it again for the content.
- **Save as…** asks where to save a copy of this version and writes it there byte for byte, as
  git stores it (`git show <commit>:<path>`). The repository is not changed.
- **Restore…** replaces the file in your working tree with this version, after you confirm. A
  file deleted since is created again. The index is not changed, so the restored content shows
  as an unstaged change you can review, stage or discard. **Undo** brings back the file as it
  was before (see [Undo and redo](undo-redo.md)). Restore is unavailable while another action,
  a merge, rebase, cherry-pick or revert is in progress.
