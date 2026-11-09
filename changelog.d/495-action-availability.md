### Action availability

- Fixed: while a rebase or `git am` started in a terminal was paused between steps, the command
  palette still offered Cherry-pick, Revert, Checkout, Reset, switching branches and other
  repository actions that the menus disable (#503).
- Fixed: the command palette offered "Stash all changes" and "Stash…", and a file's menu offered
  "Stash file…", during a rebase or a cherry-pick/revert, when the Stash button is disabled
  because stashing then loses the paused step's changes (#495).
- Changed: the command palette offers Cherry-pick and Revert for a merge commit too; they ask
  which parent to use first, like the commit menu (#495, #507).
- Changed: "Create tag at …" stays in the command palette while another operation runs, like the
  commit menu's "Create tag…"; the dialog says why it can't create the tag yet (#495).
- Changed: disabled commit menu items say why they are disabled (#495).
- Internal: menus, buttons and the command palette decide availability with one shared rule
  (`operationBlockers`, `stashBlockReason`, `commitActions`) (#495).
