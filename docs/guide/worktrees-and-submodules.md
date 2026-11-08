# Worktrees and submodules

**Worktrees** creates and removes linked worktrees, and opens one directly
in Browsitory. If the worktree's folder is inside folders that don't exist
yet, Browsitory creates them, as `git worktree add` does. On Windows, the
worktree's name and the folders in its path, as you type it, can't end in a
dot or a space,
and can't be a name Windows reserves for devices, such as `con`, `aux` or
`nul`: Browsitory refuses them before it creates anything. If creating the
worktree fails (for example, a folder can't be created, the disk is full, or
on Windows a file in the branch has a name such as `aux` or a path that's too
long), Browsitory removes what it created: the files it checked out, the
folders, the worktree entry and the new branch. The message says why it
failed, so you can fix the problem and try again with the same name. If
something else is in the new folder, Browsitory keeps the folder and names
it; delete it by hand. A half-made worktree left by an older version of
Browsitory can't be removed with Remove: delete its folder by hand, click
**Prune worktrees**, then delete the branch if you don't need it.

**Submodules** lists the repository's submodules. Initialize or update one from its row. Tick **Update
recursively** to also update the submodules inside it.

To work in a submodule, open it in its own tab: choose **Open in new tab** in its menu (the
**…** button, right-click, or the Menu key or Shift+F10 on the row's buttons), or run **Open
submodule ⟨path⟩ in new tab** from the command palette. This works once the submodule is
checked out. Until then the menu item is disabled and says why: initialize the submodule, then
update it, which checks it out. In a linked worktree, it opens that worktree's copy of the
submodule.
