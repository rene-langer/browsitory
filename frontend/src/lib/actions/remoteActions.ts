import type { OperationBlockers } from "../operationStatus";
import { gate, type PaletteAction } from "./action";

export interface RemoteActions {
  fetch: PaletteAction;
  /** Pushes the current branch to this remote. */
  pushCurrentBranch: PaletteAction;
  /** Opens the Edit remote dialog (rename, URLs). */
  edit: PaletteAction;
  /** Opens the credentials dialog, which saves the remote's sign-in mode to the repository config. */
  credentials: PaletteAction;
  /** Opens the confirmation. */
  remove: PaletteAction;
}

/**
 * Availability, reason and labels of a remote's actions (#509), shared by `BranchTree`'s remote
 * folder menu and the command palette. All of them follow the one busy rule
 * (`operationBlockers`). The edit and credentials dialogs, and the remove confirmation, check it
 * again while open and before saving (`RemoteDialogs`, #561).
 */
export function remoteActions(blockers: OperationBlockers, remoteName: string): RemoteActions {
  return {
    fetch: { label: "Fetch", paletteLabel: `Fetch ${remoteName}`, ...gate(blockers) },
    pushCurrentBranch: { label: "Push current branch here", paletteLabel: `Push to ${remoteName}`, ...gate(blockers) },
    edit: { label: "Edit remote", paletteLabel: `Edit remote ${remoteName}…`, ...gate(blockers) },
    credentials: {
      label: "Manage credentials",
      paletteLabel: `Manage credentials for ${remoteName}…`,
      ...gate(blockers),
    },
    remove: { label: "Remove remote", paletteLabel: `Remove remote ${remoteName}…`, ...gate(blockers) },
  };
}

/**
 * "Add remote…" in the Branches section's Add menu, "Add remote" in the palette. Only opens the
 * form, whose Add remote button carries the busy rule and its reason, so it is offered while an
 * operation runs (like "Create tag…", #495).
 */
export function addRemoteAction(): PaletteAction {
  return { label: "Add remote…", paletteLabel: "Add remote", enabled: true, reason: undefined };
}
