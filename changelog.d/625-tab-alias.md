### Repository tabs

- Added: "Rename tab…" in the tab menu and the command palette, and F2 on a focused tab, gives a
  tab a name of its own, for example to tell several clones or worktrees of one project apart.
  The name is kept per repository path, so it comes back after closing the tab and after a
  restart; an empty name shows the folder name again. The tab's tooltip still shows the full
  path, and the recent repositories and the workspace editor show the name too (#625).
- Added: a button at the end of the tab bar lists every open tab, grouped by workspace, with a
  filter field and the uncommitted-changes dot; Enter or a click switches to a tab. The command
  palette's "Switch to tab…" lists the other tabs by name, folder name or path (#625).
