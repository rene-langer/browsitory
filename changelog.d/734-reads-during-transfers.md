### Reads during a fetch, pull or push

- Fixed: while a fetch, pull, push or the automatic fetch was running, opening a commit, a diff,
  blame, file history, search or Refresh waited until the transfer ended; they now answer at once.
  They still wait while a pull merges and runs its `post-merge` hook (#734).
- Internal: a Tauri E2E spec opens a commit while a fetch waits on a remote that never answers,
  then cancels the fetch (#734).
