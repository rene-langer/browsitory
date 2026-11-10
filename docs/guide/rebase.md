# Rebase

Starting a rebase (from a branch or a specific commit) opens the rebase
planner as an overlay, where you can reorder/pick/squash/drop commits
before running it. Commit or stash your changes first: like `git rebase`, a rebase won't start
while tracked files have staged or unstaged changes ("You have uncommitted changes to …").
Untracked files are fine. Conflicts, once they happen, are resolved inline in the
diff pane; a paused rebase shows its progress and lets you continue or
abort.

**The planner.** It lists the commits oldest first, in the order the rebase replays them. Each
row has an action (Pick, Reword, Edit, Squash, Fixup, Drop) and **Move up**/**Move down**;
Alt+Up and Alt+Down move the row you are in. **Reword** fills a message field with the commit's
whole message, body included, for you to edit. With Squash or Fixup the group's first commit
gets a combined message field. **Start rebase** waits until the commits (and any message being
reworded) have loaded; if they can't be listed, the planner says why and offers **Retry**.

**Selecting several commits.** Ctrl/Cmd+click (mouse) toggles individual commits, which is how you
select non-adjacent ones; Shift+click or Shift+Arrow selects a range. With two or more selected, a
toolbar under the graph (and the right-click menu) offers **Interactive rebase…**, **Squash N commits**,
**Fixup N commits** and **Drop N commits**. Each opens the rebase planner, based on the parent of the oldest selected
commit and listing every commit from there to the current HEAD. The selected commits are pre-marked
with the chosen action; for Interactive rebase… they are listed as **Pick**, but are still checked
against the branch history. With Squash or Fixup the oldest selected commit stays **Pick** and the
rest fold into it. Squash and Fixup are available only for commits that sit next to each other;
Drop and Interactive rebase… work for any selection. Selections that include a merge commit or a
root commit, or commits that aren't on the current branch, can't be rebased. Esc collapses a
selection back to one commit.

**One commit, one step.** To reword, squash, fixup, move or drop a single commit, its right-click
menu runs that edit directly, without the planner (see
[Editing older commits](rewriting-commits.md)).

**A paused rebase survives restarts.** Browsitory stores a paused rebase in the repository's
`.git/browsitory/` folder, so quitting the app, closing the tab, reloading the VSCode window or a
backend crash doesn't lose it: when the repository opens again the status strip and the commit
area show the rebase where you left it, and Continue or Abort work as before. A rebase finished
after a restart is still one step for **Undo** ("Rebase onto …").

**Working on a paused rebase with git.** To the git command line a paused Browsitory rebase looks
like a cherry-pick on a detached HEAD: `git status` says "You are currently cherry-picking". Finishing
the paused step with `git cherry-pick --continue` or `git commit` is fine — Continue in Browsitory
takes that commit as the step (a reworded step still gets the message you planned) and goes on.
It does so only when the commit is recognisably that step: the step's author and message (keep the
message git suggests) and the step's changes; any other commit counts as HEAD moved.
`git cherry-pick --abort` is fine too: Continue then applies the step again.

**During a git bisect.** A rebase works while a `git bisect` started in a terminal is running
(start it from a branch: `git bisect start` alone keeps HEAD on the branch). The bisect is not
"another git operation", and finishing or aborting the rebase doesn't end it.

**During a terminal cherry-pick.** A rebase doesn't start while a `git cherry-pick A B C` (or
`git revert A B`) started in a terminal is paused. Continue or abort that first (see
[Started in a terminal](rewriting-commits.md)).

**During another operation.** A rebase doesn't start while a merge, cherry-pick or revert is
paused, even one whose conflicts you already resolved. It says which, for example "A merge is
in progress. Commit or abort it before starting a rebase."

**Rebase started in a terminal.** While a `git rebase` (or `git am`) from a terminal is paused,
the bar says **Rebasing topic, step 2 of 3 (started in a terminal)** (or **Applying patches, 1
of 3 …**) with "Continue or abort it there with git rebase --continue or git rebase --abort."
On a conflict it says "Resolve 1 conflict, then run git rebase --continue in the terminal."
It has no buttons: you continue or abort the rebase in the terminal. You can resolve the
conflicts, stage and commit in Browsitory meanwhile. A commit works as `git commit` does: at a
merge step of `git rebase --rebase-merges` it makes the merge commit, and at an empty pick it
keeps the original author. Cherry-pick, revert, merge, a new rebase, reset, amend and Abort
merge say "A rebase started in a terminal isn't finished. Continue it there with git rebase
--continue, or abort it with git rebase --abort." Abort merge refuses even a merge started by
hand during the rebase, since Browsitory can't tell it from the rebase's own merge step; use
`git merge --abort` in the terminal. Stash, amend and the sidebar's branch, tag and worktree
actions are disabled. As with git, the branch being rebased can't be deleted, renamed or
checked out in a new worktree, also from another worktree's window, and another worktree can't
switch to it.

**Interrupted rebase.** If the repository changed outside Browsitory while a rebase was paused, the
strip shows **Rebase of ⟨branch⟩ interrupted** with the reason: HEAD was moved (for example
`git switch`), the branch was changed or deleted, or another git operation is in progress. Such a
rebase can only be aborted, and **Abort rebase** then stops it without moving HEAD or touching your
branches; it only cleans up the rebase's own paused step. When the branch had changed, the toast
offers **Switch to ⟨branch⟩**, an ordinary branch switch that refuses rather than overwrite local
changes. While a rebase is paused, the branch being rebased can't be deleted, renamed or checked
out in a new worktree. The windows of the repository's other worktrees can't move, delete,
rename or switch to that branch either, or add a worktree on it ("branch 'topic' is being
rebased in another worktree at …"). Continue also refuses to finish if the
branch being rebased moved during the pause, rather than overwriting those commits; put the branch
back and Continue again, or abort and start over.
