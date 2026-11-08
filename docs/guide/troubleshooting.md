# Troubleshooting

An error stays at the top of the window until you dismiss it or start another change (stage,
commit, checkout, …); refreshing doesn't clear it.

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
