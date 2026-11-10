### Internal: workspace actions through a context

- Changed: `RepoWorkspace` builds the diff pane's and commit dock's actions once and provides
  them through `WorkspaceActionsContext` (`frontend/src/state/useWorkspaceActions.ts`).
  `DiffPane` drops from 54 props to 22 data props, and its views, file sections and the new
  `CommitDock` read the actions they need, so a new action is one member of `WorkspaceActions`
  plus its use. A required action can't be left out by mistake (a type error, where a `noop`
  default used to hide it), and a component rendered without the provider fails loudly. Tests
  build the actions with `frontend/src/test/workspaceActions.ts` (#491).
- Changed: `RepoWorkspace`'s JSX is split into `WorkspaceSidebar`, `WorkspaceMain` and
  `WorkspaceDialogs`; `RepoWorkspace.tsx` goes from 1,194 to about 610 lines (#491).
- Changed: `DiffPane.tsx` is split into one file per view (`UncommittedDiffPane`,
  `CommitDiffPane`, `StashDiffPane`, `UncommittedFileSection`), a pure move (#491).
