### Same rules for branch, remote, stash and worktree actions in menus and the palette

- Fixed: a branch's **Checkout** (and double-click) and **Rename** were only disabled during a
  rebase run here, while the command palette left out **Switch to …** during any merge,
  cherry-pick, revert, terminal rebase, action or transfer; the menu now follows the same rule
  and says why (#509).
- Fixed: a remote's **Edit remote** and **Manage credentials** stayed enabled while the
  repository was busy, though the palette left them out, and **Remove remote** gave no reason;
  all three are now disabled with the reason (#509).
- Fixed: the command palette offered **Apply stash** and **Pop stash** with staged changes or
  unresolved conflicts, which the Stashes panel refuses; it now leaves them out (#509).
- Changed: the command palette now offers **Drop stash** while a merge, rebase, cherry-pick or
  revert is paused, like the Stashes panel (#521), **Show stash** whenever a stash exists, and
  **Add remote** while the repository is busy, like the **+** menu (its form waits) (#509).
- Added: the main worktree's disabled **Remove** says why (#509).
- Internal: branch, remote, stash and worktree action labels and rules live in
  `lib/actions/*Actions.ts`, shared by the menus, panels and the palette, with an agreement test;
  `CommitGraph`'s `pending` prop is now `operationsBlocked` (#509).
