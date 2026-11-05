# Multiple repositories

Every repo you open lands in its own tab at the top of the window, independent
of the others. Only the active tab's view is kept loaded: switching back to a
tab reloads its history and changes, and a half-typed commit message is kept,
but the selected row and expanded diff sections start fresh. Open another via
**+** at the end of the tab bar. Close a tab with its **×** button, **Delete**
on the focused tab, or **Close tab** in the command palette. While a push,
fetch, or similar operation is running in the active tab, that tab can't be
closed until it finishes.
Reopening Browsitory restores every tab you had open, workspace groupings
included.
If two tabs show the same repository through different paths (for example a
subfolder, a symbolic link, or the **Open** button of a worktree), they share its
actions: one runs after the other, never both at once. They also share one
undo history, and starting a search in one stops a search still running in
the other.
