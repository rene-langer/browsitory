### Undo

- Fixed: discarding changes (a hunk, lines, a file, Discard all, restoring a file from a commit,
  applying a commit's change or a patch to the work tree) while a merge, cherry-pick, revert or
  rebase is paused, or after a stash pop that conflicted, wasn't recorded, so Undo couldn't
  bring the content back. Such a discard is now an Undo step of its own, which Undo and Redo
  run during the pause too. Reset hard and applying and staging a patch or a commit's change are
  refused then, saying why (#1147).
- Changed: a discard whose files couldn't be saved for Undo is refused instead of run, and the
  message names the file (#1147).
- Fixed: Undo of Rename branch or Delete branch brought the branch back without its upstream and
  other settings, and a rename left them behind under the new name (#1151).
- Fixed: Undo of a new branch created from another commit and switched to left that commit's
  files in the work tree (#1154).
- Fixed: Undo and Redo were refused in a sparse checkout, or with a file marked skip-worktree or
  added with intent to add (`git add -N`), whenever they changed the index. They now keep those
  marks (#1174).
- Fixed: discarding one hunk next to a big un-ignored folder saved every file of it for Undo;
  only what a discard can change is saved whole now (#1104).
- Fixed: Undo and Redo of a discard read a big file into memory whole, twice (#1089).
- Changed: the undo history file is version 3; an older Browsitory starts with an empty undo
  history instead of misreading it (#1147, #1151, #1174).
