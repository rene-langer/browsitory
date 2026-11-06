# Troubleshooting

An error stays at the top of the window until you dismiss it or start another change (stage,
commit, checkout, …); refreshing doesn't clear it.

Browsitory writes a rotated failure log (keeping the 5 most recent files) to the OS log
directory, so a bug report doesn't require a live dev session to reproduce. Attach the
newest file there to a bug report:

- **Linux:** `~/.local/share/com.browsitory.browsitory/logs`
- **macOS:** `~/Library/Logs/com.browsitory.Browsitory`
- **Windows:** `%APPDATA%\com.browsitory.Browsitory\logs`

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
