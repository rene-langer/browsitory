import { useCallback } from "react";
import type { AppState } from "./useAppState";
import { withoutDialog, type WorkspaceDialog, type WorkspaceDialogKind } from "./workspaceDialog";

export interface DialogActions {
  /** Opens `dialog` in `state.dialog`, replacing any open one (#493); no client call. */
  openDialog(dialog: WorkspaceDialog): void;
  /** Closes the open dialog; with `kind`, only when the open one is of that kind. */
  closeDialog(kind?: WorkspaceDialogKind): void;
}

export function useDialogActions(setState: (updater: (prev: AppState) => AppState) => void): DialogActions {
  const openDialog = useCallback(
    (dialog: WorkspaceDialog) =>
      // Opening the merge dialog also forgets the last merge's outcome (`state.mergeOutcome`).
      setState((prev) => ({ ...prev, dialog, ...(dialog.kind === "merge" ? { mergeOutcome: null } : {}) })),
    [setState],
  );
  const closeDialog = useCallback(
    (kind?: WorkspaceDialogKind) =>
      setState((prev) => {
        const dialog = kind === undefined ? null : withoutDialog(prev.dialog, kind);
        return dialog === prev.dialog ? prev : { ...prev, dialog };
      }),
    [setState],
  );
  return { openDialog, closeDialog };
}
