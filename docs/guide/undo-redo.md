# Undo and redo

**Ctrl+Z** (**⌘Z** on macOS) undoes the last Git operation in the active repository and
**Ctrl+Shift+Z** or Ctrl+Y (**⇧⌘Z**) redoes it. The undo/redo buttons next to Fetch/Pull/Push, the
command palette, and the toast's Undo/Redo button do the same; each names the operation it
will act on. Undo covers staging, commits, amend, reset, branch and tag changes, checkout,
cherry-pick, revert, merge, rebase, pull, every discard and restoring a file from a commit
(see [Files at a commit](files-at-a-commit.md)), so discarded or replaced changes are
recoverable. Up to 50 steps are kept, across restarts. An operation that stopped on a conflict
or an edit step (merge, cherry-pick, revert, a multi-commit sequence, rebase) becomes one undo
step when it finishes, even if you restarted the app in between; one aborted, or replaced by
another operation outside Browsitory, leaves nothing to undo. A rebase started in a terminal
is one operation through all its steps; it is continued and finished in the terminal, so it
leaves nothing to undo either. A `git bisect` started in a
terminal doesn't count as an operation in progress: operations are recorded and Undo and Redo
work during it, and they don't end it.

The notification of an operation Undo covers has an **Undo** button: a commit, a discard,
staging lines, a reset, a checkout, a fast-forward pull or merge, a cherry-pick or revert, a
history edit, a restored file and so on. It undoes that operation only: if another operation
was recorded since, it changes nothing and says "Not undone: newer changes came after this
one", and the toolbar's Undo steps back through them one by one. Operations Undo can't reverse,
such as a fetch, push or stash, offer no Undo.

Confirmations follow the same rule. Discarding a file's changes, **Discard all…**, **Delete
untracked…**, **Reset hard** and force-deleting a branch still ask first, because they drop work
in one go, and say that Undo can bring it back. Discarding a hunk or lines doesn't ask. What
Undo can't restore, such as dropping a stash or deleting a branch or tag on a remote, asks and
says it can't be undone.

Undo refuses, instead of overwriting, when the repository changed since the operation. It also
refuses when it would delete, move or switch to a branch that a worktree holds, and names it:
checked out in another worktree, being rebased (in a terminal, or paused in Browsitory in
another worktree), being bisected, or used by a cherry-pick or revert paused in Browsitory in
another worktree ("Can't undo Commit: branch 'topic' is checked out in another worktree at
'/work/topic'.", or "… has a paused cherry-pick in another worktree at …"). The step stays on
the Undo stack, so you can try again once that worktree lets go of the branch. A missing
worktree folder gets the same `git worktree prune` hint as Reflog restore. The reflog stays available for all of these cases.

Undo and Redo write each restored file next to the old one and swap it in only when it is
complete. If writing fails (for example a full disk), that file stays as it was. Files restored
before the failure keep their restored content. Fix the problem and run Undo or Redo again: it
finishes the restore. If you changed a file in the meantime, it refuses as usual. On Windows, a file another program keeps open can't be swapped: Undo and
Redo then say "'f.txt' is open in another program. Close it, then try again."

With the desktop app and the VSCode extension open on the same repository, they share one undo
history. If the other window keeps it busy for more than about 10 seconds (a pull, a commit
hook, a signing prompt), Undo, Redo, discards, restoring a file and **Reset hard** say "Another Browsitory window
is changing this repository's undo history. Try again in a moment…" and change nothing; other
operations still run, but can't be undone. After that, while the other window stays busy, each
action waits about a second for it instead of 10; a quick run of actions waits only once.
Undo, Redo, discards and **Reset hard** still wait about a second each before they say so.

Typing in a text field keeps that field's own undo. With the focus in the merge editor, the same
keys undo and redo the merge editor's own changes, not a Git operation (see
[Resolving conflicts](conflicts.md)). In the VSCode extension, use the toolbar
buttons or the command palette if Ctrl+Z is taken by VSCode.

Tools that show all refs (`git log --all`, gitk) show a "browsitory undo anchor" commit; it
keeps undoable objects from being garbage-collected.
