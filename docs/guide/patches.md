# Patches and partial reverts

Reverting or cherry-picking a commit takes all of it. To take back, or bring over, only one file
or one hunk of a commit, or to move changes around as patch files, use the actions below. The
result lands in the working tree as unstaged changes, for you to review, stage and commit (or throw
away), unless you choose to stage it, or to apply a patch file [as commits](#apply-a-patch-file).

## Revert or apply a file or a hunk of a commit

Select a commit in the graph. Each file in its diff has a **…** menu (or right-click its header):

- **Revert this file** takes back what the commit changed in that file.
- **Apply this file to the working tree** brings that file's change into your working tree, like
  a cherry-pick of just that file (handy for a commit on another branch).
- **Revert this file and stage** and **Apply this file and stage** do the same and stage the
  result too, ready to commit.
- **Save file as patch…** saves the file's change as a patch file.

Each hunk header in a commit's diff has **Revert hunk** and **Apply hunk** for that hunk only;
Shift+click either one to stage the result too. They
are disabled while **Ignore whitespace** is on (a hunk is identified with whitespace counted, as
for staging) and while another action, a merge, a rebase or a cherry-pick or revert is in
progress. A hunk is the one you see, with the [diff algorithm](history.md#diff-options) it was
shown with.

The change is applied where its lines are now, even when the file has changed above or below it
since the commit; only the changed lines and up to three lines around them have to match. When
they don't (the lines changed again since, or the file is gone), nothing is changed and the error
banner says why, the way `git apply` refuses a patch. A file the change would create that already
exists in the working tree is refused the same way. Staging the result needs the files it touches
to have no unstaged changes; the error names the first one that has, for you to stage or stash
it first, or to apply without staging.

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

**Apply patch…** in the repository toolbar or the [command palette](command-palette.md) asks for a
patch file (a plain diff or `git format-patch` mails) and shows what it holds before anything changes: each mail's
subject, author and date, and the files it changes with the lines added and removed. When the
patch doesn't apply, the preview says why (for a file with several mails, the first one is checked
against the working tree; each later one builds on the ones before it, so it is checked as it is
applied). Then choose:

- **Apply**: the changes land in the working tree, unstaged.
- **Apply and stage**: they land in the working tree and are staged (the files the patch touches
  must have no unstaged changes).
- **Apply as commits**: each mail becomes a commit on the current branch with the mail's author,
  date and message, and you as the committer (signed when your Git configuration signs commits).
  Only for `git format-patch` mails, and only with nothing staged and no merge, rebase,
  cherry-pick or revert in progress. Commit hooks don't run.

A file with several mails is applied in order, all or nothing: when one of them doesn't apply,
the changes of the ones before it are put back too (and with **Apply as commits**, their commits
are taken back), so the working tree, the staged changes and the branch are left as they were.
Each way is one step in [Undo](undo-redo.md) ("Apply patch", "Apply and stage patch" or "Apply
patch as commits"); undoing the commits moves the branch back.

In the desktop app you can also drop a `.patch`, `.diff` or `.mbox` file from your file manager
onto the window: it is applied to the working tree at once, without the preview, with the same
checks; each dropped file is one undo step.

A patch that names a file outside the repository or in `.git` (under any name a file system
reads as `.git`, such as `git~1`), would write through a symbolic link (one already in the
working tree, or one the patch itself creates), makes a symbolic link that points into `.git`
or is named `.gitmodules`, `.gitattributes`, `.gitignore` or `.mailmap`, or creates a file whose
name differs only in case from one the repository has (on a file system that ignores case) is
refused before anything is changed, whichever way you apply it, and the preview says so.
