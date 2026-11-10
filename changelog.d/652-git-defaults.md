### Git conventions by default

- Changed: a diverged pull follows your pull strategy instead of always asking: the menu next to
  **Pull** (and the command palette) offers **Pull with merge**, **Pull with rebase** and **Pull,
  fast-forward only**, remembered per repository; until then `branch.<name>.rebase`,
  `pull.rebase` and `pull.ff=only` decide, and only without any of them does the pull ask, with
  **Remember for this repository** (#652).
- Fixed: **Rebase** for a diverged pull opened the interactive planner; it now runs a plain
  rebase onto the upstream. The prompt names the branches as `origin/main`, not
  `refs/remotes/origin/main` (#652).
- Added: switching branches over local changes in the way names the files and offers **Stash and
  switch**, which can re-apply the changes on the new branch; one **Undo** goes back (#653).
- Fixed: the New branch form switched to the new branch even with **Check out after creating**
  unticked. The option now works, starts ticked and is remembered; creating and switching is one
  undo step. **Create branch…** is in the command palette and the branch switcher (#656).
- Added: committing without `user.name` and `user.email` opens a dialog to set them, for all
  repositories or this one, and then commits, instead of git's raw error (#689).
- Added: the toolbar's branch switcher lists remote branches without a local branch and checks
  one out as a new tracking branch (#726).
