### E2E reliability

- Internal: the Tauri E2E specs open a sidebar section only after the workspace's first load has
  landed, and check that it opened. A header clicked while the branch tree replaced its loading
  placeholder could miss (the worktree spec never saw its form, #824) or move far out of view
  with its body, where the windowed reflog list renders no rows (#825). The reflog spec also
  scrolls its list into view before counting entries, and both specs print the sidebar's state
  and save a screenshot when they fail.
