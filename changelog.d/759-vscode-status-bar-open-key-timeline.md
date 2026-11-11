### VSCode extension: status bar button, Open key, Timeline commits, repository list

- Added: a Browsitory button in the status bar that opens the active editor's repository; the
  `browsitory.statusBar.enabled` setting hides it (#759).
- Added: Ctrl+K Ctrl+G (Cmd+K Cmd+G on macOS) runs "Browsitory: Open" (#759).
- Added: "Show Commit in Browsitory" on a commit in VSCode's Timeline view selects it in the
  graph, loading as much history as it needs (#759).
- Changed: when "Browsitory: Open" opens the workspace's repositories and finds several, it asks
  which ones to open, all ticked. It also finds the repositories directly inside a workspace
  folder, including ones nested in another repository (#759).
