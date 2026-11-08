### Submodules

- Added: **Open in new tab** in a submodule's menu in the Submodules panel (the "…" button,
  right-click, Menu key or Shift+F10) opens the checked-out submodule as a repository tab of its
  own, like a worktree; also in the command palette as **Open submodule ⟨path⟩ in new tab**. For
  a submodule that isn't checked out yet the item is disabled and says why, and the menu offers
  **Initialize** and **Update** instead. In a linked worktree it opens that worktree's copy of
  the submodule (#445).
