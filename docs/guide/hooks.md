# Hooks

Browsitory runs your repository's client-side git hooks the way git does:

| Hook | Runs when you… |
|---|---|
| `pre-commit`, `prepare-commit-msg`, `commit-msg`, `post-commit` | commit or amend, or **Continue** a paused multi-commit cherry-pick or revert |
| `pre-merge-commit`, `prepare-commit-msg`, `commit-msg` | commit a merge that had no conflicts (instead of `pre-commit`; no `post-commit`; the message hooks get `.git/MERGE_MSG`, as with `git merge`) |
| `prepare-commit-msg`, `post-commit` | cherry-pick or revert: for each commit Browsitory creates from a step that applied without conflicts |
| `pre-push` | push a branch, force push, push tags, **Push to…**, delete a remote branch or a tag on a remote |
| `post-checkout` | switch branches or check out a commit |
| `post-merge` | after a merge commit, a fast-forward or a squash, or a pull with a fast-forward |

Hooks come from `core.hooksPath` (a relative path is relative to the repository root) or
`.git/hooks`. As in git, this includes a `core.hooksPath` set in the repository's own config;
only open repositories you trust. (External diff and merge tools are stricter, see
[Diff and merge tools](external-tools.md#diff-and-merge-tools).) They run in the repository root with the same arguments and standard input git
gives them, and like git Browsitory does not set `GIT_DIR`, so a hook can `cd` into a
subdirectory (for example `cd frontend && npx lint-staged`) and run git there. On macOS
and Linux a hook must be executable. Hook managers such as husky, pre-commit or lefthook work
through the hook scripts they install. Files a `pre-commit` hook stages (formatters,
lint-staged) are part of the commit. `pre-push` runs after Browsitory has contacted the remote
and before anything is sent, so it sees the remote's current commits, as in git.

A failing `pre-commit`, `pre-merge-commit`, `commit-msg` or `pre-push` stops the commit or push. A dialog shows the
hook's output; **Commit without hooks**, **Amend without hooks** or **Push without hooks**
retries with those hooks skipped (`--no-verify`). A failing `prepare-commit-msg` cannot be
skipped, so its dialog only has **Close**. Close the dialog and **Show hook output…** in the
error banner brings it back. Your commit message stays in the box until a commit succeeds.
Failures of `post-commit`, `post-checkout` and `post-merge` are ignored, as in git.

To skip hooks up front, tick **Skip hooks** under the commit message (shown only when a
`pre-commit` or `commit-msg` hook, or while a merge waits to be committed a `pre-merge-commit`
hook, is installed) or run "Skip hooks for the next commit" from the palette. It applies to the
next commit or amend only.

While a commit hook runs, or `post-merge` after you commit a merge that had no conflicts,
"Running pre-commit hook…" (with the hook's name) shows under the commit message with **Cancel**. Cancelling
`post-commit` or `post-merge` stops the hook and keeps the commit, and **Skip hooks** doesn't skip
`post-merge` (as with `git merge --no-verify`). While `post-checkout` runs after you switch
branches or check out a commit, `post-merge` after a fast-forward or squash merge, or a
cherry-pick's or revert's hooks, "Running post-checkout hook…" shows in a strip at the top of the
repository, with **Cancel**. Cancelling stops the hook and keeps the switch,
merge or commit. A running `pre-push` shows in the transfer panel, whose Cancel stops it; so
does `post-merge` after a pull that fast-forwarded ("Running post-merge hook…"), whose Cancel
stops only the hook and keeps the pull. Browsitory
stops any hook after 10 minutes. Other actions on the repository wait while a hook runs.

Hooks cannot ask you anything: their input is empty (or, for `pre-push`, the list of refs), and
they run without a terminal, even when you started Browsitory from one. A hook that tries to
read from the terminal (`/dev/tty`) fails, and its error shows in the dialog. If a hook leaves a background process running that keeps its output open, Browsitory
waits up to 2 more seconds for output after the hook exits, then carries on.

Like signing, apps started from the macOS Dock may not see your shell's `PATH`. If a hook cannot
find a tool, use absolute paths in the hook or extend `PATH` inside it.

On Windows, hooks run with the `sh.exe` that comes with Git for Windows (found next to `git.exe`
or on `PATH`). Without it, a `pre-commit`, `commit-msg` or `pre-push` hook fails with a message
saying so (use the "without hooks" retry to go ahead), and other hooks are skipped. Cancel and
the time limit stop the hook and the programs it started.

Rebase, undo/redo, stash, worktree creation and clone run no hooks.
Finishing a paused merge, cherry-pick or revert with Commit runs the commit hooks. The commit
that concludes a merge Browsitory staged without conflicts runs what `git merge` runs:
`pre-merge-commit` instead of `pre-commit`, `.git/MERGE_MSG` for `prepare-commit-msg` and
`commit-msg`, no `post-commit`, and `post-merge` once the commit exists (if Browsitory was
restarted in between, it runs the ordinary commit hooks, as `git merge --no-commit` followed by
`git commit` does). If the merge is aborted and started again in a terminal while
`pre-merge-commit` runs, nothing is committed and Browsitory says "The merge changed while its
hooks ran."; check the merge and commit again.

A cherry-pick or revert that applies without conflicts runs what `git cherry-pick` runs for
each commit it creates: `prepare-commit-msg` (with `COMMIT_EDITMSG` and `message`; it may rewrite
the message) and `post-commit`, but no `pre-commit` or `commit-msg`. This holds for a single
pick, every step of a multi-commit cherry-pick or revert, and the steps **Continue** applies after
the paused one. If `prepare-commit-msg` fails (or empties the message), that commit is not made:
it stays staged and paused, as with git. The dialog shows the hook's output (with **Close**
only: the hook can't be skipped); fix the hook and **Continue**, or commit it from the commit
box, or abort. **Cancel** during a step's `post-commit` keeps that commit and stops the
multi-commit cherry-pick or revert after it ("Stopped after …; its commit was kept. Continue to
resume."); **Continue** goes on with the next step.
**Continue** on a paused multi-commit cherry-pick or revert runs the commit hooks for the step it
commits (the one you resolved). If one fails, the step stays paused and the dialog offers
**Continue without hooks**; a running hook shows on the panel with **Cancel**.

If a commit was created but git's merge or cherry-pick state could not be cleared afterwards
(for example a read-only `.git` folder), the commit still counts and its `post-commit` or
`post-merge` runs; the leftover state stays in the operation strip, and **Abort** clears it
without changing the commit. A multi-commit cherry-pick or revert counts that step too: it
stops on the next step if that one can't be applied either, and **Continue** clears the
leftover state and goes on. The next commit after a leftover merge state clears it first, so it
doesn't become a second merge commit. The next commit after a leftover cherry-pick state also
clears it first, so you are the commit's author, not the author of the picked commit. If the
state still can't be cleared, the commit is refused until you fix the permissions of the `.git`
folder.
