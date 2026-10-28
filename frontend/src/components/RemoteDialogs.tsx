import { useEffect, useRef, useState } from "react";
import type { RemoteAuthMode, RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";
import type { RemoteDialog } from "../state/useAppState";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { EditRemoteDialog } from "./EditRemoteDialog";
import { RemoteCredentialsDialog } from "./RemoteCredentialsDialog";

/**
 * The remote folder menu's dialogs (Edit / Credentials / Remove), driven by `AppState.remoteDialog`.
 * `App` renders this at its own level, not inside the sidebar: a collapsed sidebar stays mounted
 * but `hidden`, and `showModal()` on a dialog in a `display: none` subtree opens a 0×0 modal that
 * leaves the whole page inert.
 *
 * When the dialog's remote disappears from `remotes` (removed or renamed by something else, seen on
 * a refresh) the dialog closes and `onRemoteGone` reports it — except while the dialog's own rename
 * or remove is running: those update `remotes` optimistically before the request settles (and roll
 * back on failure), so the dialog keeps a snapshot of its remote and stays mounted with what the
 * user typed.
 */
export function RemoteDialogs({
  remoteDialog,
  remotes,
  remoteUpstreams,
  operationDisabled,
  onRenameRemote,
  onUpdateRemoteUrls,
  onRemoveRemote,
  onSaveHttpsCredential,
  onForgetHttpsCredential,
  onSetRemoteAuthMode,
  onClose,
  onRemoteGone,
}: {
  remoteDialog: RemoteDialog | null;
  remotes: RemoteInfo[];
  remoteUpstreams: Record<string, UpstreamInfo[]>;
  operationDisabled: boolean;
  onRenameRemote: (oldName: string, newName: string) => Promise<boolean>;
  onUpdateRemoteUrls: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<void>;
  onRemoveRemote: (name: string, clearUpstreams: boolean) => Promise<void>;
  onSaveHttpsCredential: (remoteName: string, username: string, token: string) => Promise<void>;
  onForgetHttpsCredential: (remoteName: string) => Promise<void>;
  onSetRemoteAuthMode: (remoteName: string, mode: RemoteAuthMode, username: string | null) => Promise<boolean>;
  onClose: () => void;
  /** The open dialog's remote is no longer listed (and the dialog did not remove or rename it). */
  onRemoteGone?: (remoteName: string) => void;
}) {
  const liveRemote = remoteDialog === null ? undefined : remotes.find((r) => r.name === remoteDialog.remoteName);
  const [snapshot, setSnapshot] = useState<{ dialog: RemoteDialog; remote: RemoteInfo } | null>(null);
  if (remoteDialog === null) {
    if (snapshot !== null) setSnapshot(null);
  } else if (liveRemote !== undefined && (snapshot?.dialog !== remoteDialog || snapshot.remote !== liveRemote)) {
    setSnapshot({ dialog: remoteDialog, remote: liveRemote });
  }
  const remote = liveRemote ?? (snapshot !== null && snapshot.dialog === remoteDialog ? snapshot.remote : undefined);

  // The dialog whose own rename/remove is running (or succeeded and is about to close): its
  // remote missing from `remotes` is expected then, not an outside change.
  const ownMutation = useRef<RemoteDialog | null>(null);
  const reported = useRef<RemoteDialog | null>(null);
  // Latest callbacks, so the check below runs only when the dialog or its remote changes.
  const callbacks = useRef({ onClose, onRemoteGone });
  useEffect(() => {
    callbacks.current = { onClose, onRemoteGone };
  });
  useEffect(() => {
    if (remoteDialog === null) {
      ownMutation.current = null;
      return;
    }
    if (liveRemote !== undefined || ownMutation.current === remoteDialog || reported.current === remoteDialog) return;
    reported.current = remoteDialog;
    callbacks.current.onClose();
    callbacks.current.onRemoteGone?.(remoteDialog.remoteName);
  }, [remoteDialog, liveRemote]);

  if (remoteDialog === null || remote === undefined) return null;
  const dialog = remoteDialog;

  if (dialog.kind === "remove") {
    // Computed now, not when the menu item was clicked: upstreams may change meanwhile.
    const affected = remoteUpstreams[remote.name] ?? [];
    const clearUpstreams = affected.length > 0;
    return (
      <ConfirmDialog
        ariaLabel="Remove remote confirmation"
        message={
          clearUpstreams ? (
            <p>
              Remove {remote.name} and clear upstreams for {affected.map((item) => item.localBranch).join(", ")}?
            </p>
          ) : (
            <p>Remove remote {remote.name}?</p>
          )
        }
        confirmLabel="Confirm remove"
        confirmDisabled={operationDisabled}
        onConfirm={() => {
          ownMutation.current = dialog;
          void onRemoveRemote(remote.name, clearUpstreams).then(onClose);
        }}
        onCancel={onClose}
      />
    );
  }
  if (dialog.kind === "edit") {
    return (
      <EditRemoteDialog
        key={dialog.remoteName}
        remote={remote}
        onRename={async (oldName, newName) => {
          ownMutation.current = dialog;
          const renamed = await onRenameRemote(oldName, newName);
          // A failed rename rolled `remotes` back: outside changes count again.
          if (!renamed && ownMutation.current === dialog) ownMutation.current = null;
          return renamed;
        }}
        onUpdateUrls={onUpdateRemoteUrls}
        onClose={onClose}
      />
    );
  }
  return (
    <RemoteCredentialsDialog
      key={dialog.remoteName}
      remote={remote}
      onSetAuthMode={onSetRemoteAuthMode}
      onSaveHttpsCredential={onSaveHttpsCredential}
      onForgetHttpsCredential={onForgetHttpsCredential}
      onClose={onClose}
    />
  );
}
