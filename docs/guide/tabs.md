# Multiple repositories

Every repo you open lands in its own tab at the top of the window, independent
of the others. Only the active tab's view is kept loaded: switching back to a
tab reloads its history and changes, and a half-typed commit message is kept,
but the selected row and expanded diff sections start fresh. Open another via
**+** at the end of the tab bar, or (desktop app) drop its folder on the window. Close a tab with its **×** button (dimmed on the other tabs), **Delete**
on the focused tab, or **Close tab** in the command palette. While a push,
fetch, or similar operation is running in the active tab, that tab can't be
closed until it finishes.
To bring a closed tab back, use **Reopen closed tab** in the command palette or in a
tab's menu (right-click), or press **Ctrl+Shift+T** (**⇧⌘T** on macOS) where the host doesn't
use that key. It returns to its old place and workspace group and becomes the active tab;
repeat it to go further back (up to ten tabs, until you quit). A repository that
can no longer be opened is skipped, with a message saying why.
A dot after a tab's name means its repository has uncommitted changes: staged,
unstaged or untracked files, as the **Uncommitted changes** row counts them. The
active tab's dot follows its changes live; the other tabs are checked when you
leave them, when the window comes back into view, and after a fetch.
Reopening Browsitory restores every tab you had open, workspace groupings
included.

## Naming a tab

A tab is named after its repository's folder. To give it a name of its own, for
example to tell several clones or worktrees of one project apart, choose **Rename
tab…** in the tab's menu (right-click) or in the command palette, or press **F2**
on the focused tab. Leave the name empty to show the folder name again. The name
belongs to the repository's path: it comes back when you close and reopen the
tab, after a restart, and it is also shown in the recent repositories and the
workspace editor. Hovering a tab still shows its full path.

## Finding a tab

When many tabs are open, the **⌄** button at the end of the tab bar (next to
**+**) lists them all, grouped by workspace, with the uncommitted-changes dot
where it applies. Type to filter by name, folder name or path, move with
**↑**/**↓**, and press **Enter** or click to switch; **Esc** closes the list. In
the command palette, **Switch to tab…** does the same for the other open tabs.
From anywhere in the window, **Ctrl+Tab** and **Ctrl+Shift+Tab** (**⌃⇥** and **⌃⇧⇥** on
macOS) switch to the next and previous tab, and **Ctrl+1** … **Ctrl+8** (**⌘1** … **⌘8**) to
the first eight tabs, **Ctrl+9** (**⌘9**) to the last, where the host doesn't take those keys
(see [Keyboard shortcuts](command-palette.md#keyboard-shortcuts)).
If two tabs show the same repository through different paths (for example a
subfolder, a symbolic link, or the **Open** button of a worktree), they share its
actions: one runs after the other, never both at once. They also share one
undo history, and starting a search in one stops a search still running in
the other.
