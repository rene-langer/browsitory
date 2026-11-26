### Branch and remote settings, merge settings without a value

- Fixed: Delete branch removed, and Rename branch moved, the settings of another branch whose name
  continues after a dot (deleting `release` lost `release.1`'s upstream and description), and
  both failed halfway on a branch setting in your own git config. They now change exactly the
  settings `git branch -D` and `git branch -m` change, in the repository's own config (#1181).
- Fixed: Rename remote and Remove remote did the same to a remote named like `origin.mirror`,
  Rename remote wrote Browsitory's own sign-in settings of the remote under a stray remote, failed
  halfway on a remote with several fetch refspecs, and left `pushRemote` and `remote.pushDefault`
  naming the old name; Remove remote deleted local branches the remote's push refspec named and
  could take the upstream settings of the wrong branch. Both now do what `git remote rename` and
  `git remote rm` do (#1181).
- Fixed: A `merge.default` or `merge.conflictstyle` setting written without a value (the name
  alone in a config file) crashed the app's backend (or the VS Code extension's helper) on every
  merge, pull, cherry-pick, revert, rebase, stash, switch, reset, discard, patch apply, Undo of a
  work-tree change and worktree add. These commands now refuse and name the setting, as git does,
  without changing anything (#1182).
