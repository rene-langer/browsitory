# Patches and partial reverts

Reverting or cherry-picking a commit takes all of it. To take back, or bring over, only one file
or one hunk of a commit, or to move changes around as patch files, use the actions below. None of
them commits: the result lands in the working tree as unstaged changes, for you to review, stage
and commit (or throw away).

## Revert or apply a file or a hunk of a commit

Select a commit in the graph. Each file in its diff has a **…** menu (or right-click its header):

- **Revert this file** takes back what the commit changed in that file.
- **Apply this file to the working tree** brings that file's change into your working tree, like
  a cherry-pick of just that file (handy for a commit on another branch).
- **Save file as patch…** saves the file's change as a patch file.

Each hunk header in a commit's diff has **Revert hunk** and **Apply hunk** for that hunk only. They
are disabled while **Ignore whitespace** is on (a hunk is identified with whitespace counted, as
for staging) and while another action, a merge, a rebase or a cherry-pick or revert is in
progress.

The change is applied where its lines are now, even when the file has changed above or below it
since the commit; only the changed lines and up to three lines around them have to match. When
they don't (the lines changed again since, or the file is gone), nothing is changed and the error
banner says why, the way `git apply` refuses a patch. A file the change would create that already
exists in the working tree is refused the same way.

Every revert or apply is one step in [Undo](undo-redo.md) ("Revert hunk in a.txt from 1234567",
for example).

## Save as patch

- **A commit**: right-click it in the graph and choose **Save as patch…**.
- **Several commits**: select them (Ctrl/Cmd+click or Shift+click) and choose **Save N commits as
  patch…** from their menu or the bar below the graph. The file holds one `git format-patch` mail
  per commit, oldest first, so `git am` can turn them back into commits.
- **A file of a commit**: **Save file as patch…** in the file's menu (a plain diff).
- **The staged changes**: **Save as patch…** next to **Unstage all** in the Staged heading, or
  **Save staged changes as patch…** in the [command palette](command-palette.md).

Browsitory asks where to save the file and suggests a name (`0001-<subject>.patch` for one
commit). Binary files are included.

## Apply a patch file

**Apply patch…** in the [command palette](command-palette.md) asks for a patch file (a plain diff
or `git format-patch` mails) and applies it to the working tree. A file with several mails is
applied in order, all or nothing: when one of them doesn't apply, the changes of the ones before
it are put back too and the working tree is left as it was. Applying a patch never commits and is
one step in [Undo](undo-redo.md) ("Apply patch").

A patch that names a file outside the repository or in `.git`, would write through a symbolic
link, or creates a file whose name differs only in case from one the repository has (on a file
system that ignores case) is refused before anything is changed.
