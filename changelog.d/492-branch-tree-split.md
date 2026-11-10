### Internal: BranchTree split by job

- Changed: `BranchTree` (1,288 lines, 28 `useState`) is split by job, as pure moves: the tree
  rows, folders and graph swatches in `BranchTreeView`, the inline forms and the upstream dialog
  in `NewBranchForm`, `AddRemoteForm` and `SetUpstreamDialog` (each owns its fields), the lazy
  remote branch lists in `useRemoteBranchCache`, and the context menu items in
  `branchMenuItems.ts`, like `tagMenuItems.ts`. Its props and behaviour are unchanged; the tests
  are split to match (#492).
