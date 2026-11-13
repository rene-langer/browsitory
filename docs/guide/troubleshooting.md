# Troubleshooting

An error stays at the top of the window until you dismiss it or start another change (stage,
commit, checkout, …); refreshing doesn't clear it. **Show details** opens the
[operation log](operation-log.md) on the failed operation: what it was given, its full error and
the output of the hooks it ran.

The desktop app writes a log file, `browsitory.log`, to your system's log folder, so a bug
report doesn't require a live dev session to reproduce. When the file reaches 40 KB, it is
renamed with the date and time (`browsitory_2026-11-06_14-30-00.log`) and a new one starts; the
5 most recent old files are kept. Attach `browsitory.log` (and the newest old file, if the
problem was earlier) to a bug report:

- **Linux:** `~/.local/share/com.browsitory.desktop/logs` (or
  `$XDG_DATA_HOME/com.browsitory.desktop/logs` if you set `XDG_DATA_HOME`)
- **macOS:** `~/Library/Logs/com.browsitory.desktop`
- **Windows:** `%LOCALAPPDATA%\com.browsitory.desktop\logs`

Up to version 0.3.x these folders were named `com.browsitory.app`. The first start of 0.4.0 or
later renames them, and the log file then names each folder it moved. If a folder could not be
moved, the log says why and the old folder is left as it was.

Report problems on [GitHub](https://github.com/rene-langer/browsitory/issues).

## "Couldn't show …"

If a bug stops Browsitory from drawing a repository's tab, the tab shows "Couldn't show" with the
repository's name and the error instead of going blank. The other tabs and the rest of the window
keep working. **Reload tab** reads the repository again (your commit message draft stays),
**Close tab** closes it, and **Copy details** copies the version and the error for a bug report.
If the window itself can't be drawn, it shows "Browsitory couldn't show this window" with
**Reload**, which opens your repositories again. Either way the error also goes to the log.

## An invalid `core.ignorecase`

If the repository's `core.ignorecase` setting holds something other than true or false,
Browsitory can't tell whether names that differ only in case are the same file. Actions that
write files from a commit then stop before they change anything: switching branches, merging,
pulling, cherry-picking, rebasing, saving, applying or popping a stash, resetting, discarding
changes, Abort, undo/redo, cloning, adding a worktree and updating a submodule. Dropping a
stash still works. The error names the setting, for example "Can't read core.ignorecase from
the Git config: failed to parse 'maybe' as a boolean." Set it to true or false, or remove it
(`git config --unset core.ignorecase`),
then try again.

## Files whose names differ only in case

On Linux, with `core.ignorecase` on, Git keeps only one of two files whose names differ only in
case, such as `A` and `a`. Browsitory therefore refuses, before it changes anything, the actions
that would lose one of them: switching to a branch or checking out a commit, tag or remote branch
that has such a pair the current commit doesn't, adding such a worktree, a merge, pull, rebase
step, cherry-pick, revert or stash apply that brings one in, a hard reset, starting a rebase and
discarding one file of the pair. The error names both files. Rename one of them in a commit, then
try again. Aborting a rebase, cherry-pick or revert always works, even with such a pair.
