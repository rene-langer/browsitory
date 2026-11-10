### Repository tabs

- Added: a tab whose repository has uncommitted changes (staged, unstaged or untracked) shows a
  dot after its name, also announced as "Uncommitted changes". The active tab follows its live
  status; the other tabs are checked when you leave them, when the window comes back into view
  and after a fetch (#613).
- Added: "Reopen closed tab" in the command palette and the tab menu, and Ctrl/Cmd+Shift+T where
  the host doesn't use the key. It brings back the most recently closed tab at its old place and
  in its workspace group; repeat it to go further back, up to ten tabs per session. A repository
  that can no longer be opened is skipped with a message (#613).
