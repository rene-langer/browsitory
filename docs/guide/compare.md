# Comparing commits and branches

Compares are read-only and replace the diff pane until you click **×** or click a row (a
commit, a stash, a search result), including the one already selected.

- **Two commits:** select one commit, then Ctrl-click (Cmd-click on macOS) a second one, and
  choose **Compare these commits** in the bar under the graph or in the right-click
  menu. The older commit (lower in the graph) is on the left: the header reads
  `Comparing older..newer`. **Swap** reverses it. The commits do not need to be related; this
  is `git diff A B`.
- **A branch since it diverged:** right-click a local branch other than the current one, or a
  remote-tracking branch, and choose **Compare with current branch** (**Compare with HEAD**
  when no branch is checked out). It shows what the branch changed since its merge base with
  HEAD (`git diff HEAD...branch`), so commits only on your branch do not appear.
- **A commit against the working tree:** right-click a commit and choose **Diff against working
  tree**. Staged, unstaged and untracked changes are all included, and the view follows edits on
  disk.

A file git detects as renamed is listed once with "Renamed from …", and its diff shows only
the edits. In a working-tree compare that holds for renames git already tracks (`git mv`, or a
move you staged); a file moved on disk but not yet added shows as a deletion plus a new
untracked file. Each
file has **Blame** (at the newer side, or at HEAD for the working tree) and **History**. With
more than 50 files, every file starts collapsed; use **Expand all**.
