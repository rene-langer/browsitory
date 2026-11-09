### Busy rule gaps

- Fixed: the New branch form's **Create** was enabled during a merge, a cherry-pick or revert, a
  rebase started in a terminal, another action or a transfer; only a rebase run here disabled it.
  It now follows the same rule as the other branch actions and says why (#561).
- Fixed: **Edit remote** and **Manage credentials** saved even when the repository became busy
  after they opened; they now disable saving and forgetting, and say why (#561).
- Fixed: the Remove remote and Remove worktree confirmations now say why they can't remove while
  the repository is busy (#561).
- Added: the command palette's **Open main worktree**, like the Worktrees panel's Open for the main
  worktree (#561).
