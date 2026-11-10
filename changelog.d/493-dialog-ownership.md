### Internal: one owner per dialog

- Changed: the nine modal dialogs `AppState` kept in fields of their own (push, force push,
  delete remote branch, create tag, delete remote tag, a remote's edit / credentials / remove,
  merge, reword, stash) are one `dialog: WorkspaceDialog | null` union with one `openDialog` and
  one `closeDialog`, replacing 17 open/close actions; an action that finishes closes only its own
  dialog. The inline New branch and Add remote forms keep their own fields, since they can be open
  together and next to a dialog. `docs/ARCHITECTURE.md`'s "Who owns a dialog's open state" now
  says which dialogs live in `AppState`, in `RepoWorkspace` or in the component (#493).
- Changed: `BranchTree` and `DiffPane` no longer switch between local and lifted dialog state:
  `BranchTree` asks its owner to open a remote dialog (`onOpenRemoteDialog`) and no longer renders
  one, and `DiffPane` always takes `discardRequest` from its owner. Their tests hold the state in a
  small wrapper, as the workspace does (#493).
