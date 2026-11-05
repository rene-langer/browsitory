# Reflog

**Reflog** lists recent HEAD movements per reference and can restore a
branch to an earlier entry — useful for recovering from a bad reset or
rebase. Restore moves only the branch: your files and staged changes stay as they are.

Like `git branch -f`, Restore refuses a branch that another worktree has checked out
("branch 'topic' is checked out in another worktree at '/work/topic'"), and a branch that a
rebase started in a terminal is rebasing, in any worktree. Restore it from that worktree, or
finish or abort the rebase first. It also refuses:

- the branch a `git bisect` started from, in any worktree ("branch 'topic' is being bisected
  …"). End the bisect with `git bisect reset` first.
- a branch that a `git rebase --update-refs` in another worktree will move.
- the branch a rebase paused in Browsitory is rebasing in another worktree ("branch 'topic' is
  being rebased in another worktree at …"). Continue or abort that rebase first.
- the branch a cherry-pick or revert paused in Browsitory runs on in another worktree ("branch
  'topic' has a paused cherry-pick in another worktree at …"). Continue or abort it first.

If that other worktree's folder is gone, or is no longer a worktree, the message says so and
asks you to run `git worktree prune`, which removes the stale entry. If that worktree is locked
(for example on an unmounted drive), git keeps it, and the message says so: run `git worktree
unlock` first if it is gone for good. Like Reset, Restore doesn't move this worktree's HEAD or current branch while a
merge, cherry-pick, revert or rebase is paused ("Finish or abort the current
merge/cherry-pick/revert/rebase first."; for a rebase started in a terminal, it says to
continue or abort it there).
