import type { PatchContent, PatchPreview } from "../ipc/RepoClient";

/**
 * The workspace's one open modal dialog (#493), in `AppState.dialog`. These are the dialogs that
 * more than one place opens (a menu and the command palette, say) or that an action opens or
 * closes itself (a push that needs a target, a stash that landed), so they live in app state;
 * see `docs/ARCHITECTURE.md`'s "Who owns a dialog's open state". At most one is open: opening one
 * replaces whatever was open.
 *
 * Not here: the sidebar's inline New branch and Add remote forms (`createBranchDraft`,
 * `addRemoteDraftOpen`), which are not modal and can be open together and next to a dialog.
 */
export type WorkspaceDialog =
  // "Push to…" for that local branch.
  | { kind: "push"; localBranch: string }
  // The force-push (with lease) confirmation, from the error banner's offer.
  | { kind: "forcePush"; remoteName: string }
  // Delete a remote branch; both null is the palette's picker mode.
  | { kind: "deleteRemoteBranch"; remoteName: string | null; branchName: string | null }
  // Create a tag on `commitId`.
  | { kind: "createTag"; commitId: string }
  // "Delete from remote" for that tag.
  | { kind: "deleteRemoteTag"; tagName: string }
  // A remote's Edit / Credentials / Remove dialog (`RemoteDialogs`).
  | { kind: "remote"; action: "edit" | "credentials" | "remove"; remoteName: string }
  // The merge options dialog for that branch.
  | { kind: "merge"; branchName: string }
  // Reword an older commit (#442); `onto` is its parent.
  | { kind: "reword"; commitId: string; onto: string }
  // The stash options dialog; `paths` non-empty for a file stash.
  | { kind: "stash"; paths: string[] }
  // "Apply patch…"'s preview of the patch file picked (#658): Apply, Apply and stage, Apply as
  // commits.
  | { kind: "applyPatch"; content: PatchContent; preview: PatchPreview };

export type WorkspaceDialogKind = WorkspaceDialog["kind"];

export type DialogOf<K extends WorkspaceDialogKind> = Extract<WorkspaceDialog, { kind: K }>;

/** A remote's Edit / Credentials / Remove dialog, opened from its menu or the palette. */
export type RemoteDialog = DialogOf<"remote">;

/** The open dialog when it is a `kind` one (the same object, so its identity is kept), else `null`. */
export function dialogOf<K extends WorkspaceDialogKind>(dialog: WorkspaceDialog | null, kind: K): DialogOf<K> | null {
  return dialog !== null && dialog.kind === kind ? (dialog as DialogOf<K>) : null;
}

/**
 * `dialog` with a `kind` one closed and any other left open: an action that finishes closes its
 * own dialog, never one opened meanwhile.
 */
export function withoutDialog(dialog: WorkspaceDialog | null, kind: WorkspaceDialogKind): WorkspaceDialog | null {
  return dialog !== null && dialog.kind === kind ? null : dialog;
}
