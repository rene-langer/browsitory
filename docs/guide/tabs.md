# Multiple repositories

Every repo you open lands in its own tab at the top of the window, independent
of the others. The last four tabs you looked at stay loaded: switching back to
one of them shows it as you left it, with the same selected commit, scroll
positions, open panels, search and file history, and reads the repository again
only if it changed meanwhile. A tab you haven't looked at for longer starts
fresh and reloads its history and changes; its half-typed commit message is
still kept. Open another via
**+** at the end of the tab bar, or (desktop app) drop its folder on the window. Close a tab with its **×** button (dimmed on the other tabs), **Delete**
on the focused tab, or **Close tab** in the command palette. While a push,
fetch, or similar operation is running in the active tab, that tab can't be
closed until it finishes.
With a tab focused, **Left**/**Right**, **Home** and **End** move the focus to
another tab without switching to it; **Enter** or **Space** switches.
To bring a closed tab back, use **Reopen closed tab** in the command palette or in a
tab's menu (right-click), or press **Ctrl+Shift+T** (**⇧⌘T** on macOS) where the host doesn't
use that key. It returns to its old place and workspace group and becomes the active tab;
repeat it to go further back (up to ten tabs, until you quit). A repository that
can no longer be opened is skipped, with a message saying why.
A dot after a tab's name means its repository has uncommitted changes: staged,
unstaged or untracked files, as the **Uncommitted changes** row counts them. The
active tab's dot follows its changes live; the other tabs are checked when you
leave them, when the window comes back into view, and after a fetch.

## Changes made outside Browsitory

You don't need to refresh after working in a terminal, an editor or another Git
tool. Browsitory asks your operating system to tell it when files of the
repository change, and then checks whether anything it displays has changed:
files, staged changes, commits, branches, tags, stashes, remotes, or a merge or
rebase in progress. When something has, it reloads the tab, usually within a
second; when nothing has, it reads nothing more. It also checks at once when the
window comes into view (in VSCode, when the panel is shown again), when you
switch to a tab, and every 30 seconds in case a change went unreported (some
network drives report none). Files your `.gitignore`
ignores don't count, and nothing is checked while one of the tab's own actions
runs: its own reload follows when it is done. A tab in the background is checked
when you switch to it.

Where the operating system can't report changes, Browsitory checks every few
seconds instead: on some network drives, when Linux has no file watches left
(each folder of a repository needs one; see `fs.inotify.max_user_watches`), and
in a repository with more than 20 000 folders that aren't ignored. In a very
large repository, where a check takes longer, those checks come less often (at
most one a minute). It then also checks when the window gets the focus back.
The **Refresh** command still reloads at once.
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
