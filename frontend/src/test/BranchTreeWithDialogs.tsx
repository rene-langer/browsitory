import { useState } from "react";
import { BranchTree } from "../components/BranchTree";
import { RemoteDialogs } from "../components/RemoteDialogs";
import type { RemoteDialog } from "../state/workspaceDialog";

type RemoteDialogsProps = Parameters<typeof RemoteDialogs>[0];

/** `BranchTree`'s props plus the remote dialogs' own, for `BranchTreeWithDialogs`. */
export type BranchTreeProps = Parameters<typeof BranchTree>[0] &
  Pick<
    RemoteDialogsProps,
    | "remoteUpstreams"
    | "onRenameRemote"
    | "onUpdateRemoteUrls"
    | "onRemoveRemote"
    | "onSaveHttpsCredential"
    | "onForgetHttpsCredential"
    | "onSetRemoteAuthMode"
  >;

/**
 * `BranchTree` with the remote dialogs it opens, as the workspace has them (#493): the open dialog
 * is held here, as `AppState.dialog` holds it, and `RemoteDialogs` renders next to the tree, as in
 * `WorkspaceDialogs`. `onOpenRemoteDialog` still sees every open request.
 */
export function BranchTreeWithDialogs(props: BranchTreeProps) {
  const [remoteDialog, setRemoteDialog] = useState<RemoteDialog | null>(null);
  const {
    remoteUpstreams,
    onRenameRemote,
    onUpdateRemoteUrls,
    onRemoveRemote,
    onSaveHttpsCredential,
    onForgetHttpsCredential,
    onSetRemoteAuthMode,
    ...treeProps
  } = props;
  return (
    <>
      <BranchTree
        {...treeProps}
        onOpenRemoteDialog={(dialog) => {
          props.onOpenRemoteDialog(dialog);
          setRemoteDialog(dialog);
        }}
      />
      <RemoteDialogs
        remoteDialog={remoteDialog}
        remotes={props.remotes}
        remoteUpstreams={remoteUpstreams}
        blockedReason={props.operationDisabled ? props.operationDisabledReason : null}
        onRenameRemote={onRenameRemote}
        onUpdateRemoteUrls={onUpdateRemoteUrls}
        onRemoveRemote={onRemoveRemote}
        onSaveHttpsCredential={onSaveHttpsCredential}
        onForgetHttpsCredential={onForgetHttpsCredential}
        onSetRemoteAuthMode={onSetRemoteAuthMode}
        onClose={() => setRemoteDialog(null)}
      />
    </>
  );
}
