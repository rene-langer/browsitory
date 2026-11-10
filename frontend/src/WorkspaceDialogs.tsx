import { useCallback } from "react";
import { CommandPalette } from "./components/CommandPalette";
import { CreateTagDialog } from "./components/CreateTagDialog";
import { DeleteRemoteBranchDialog } from "./components/DeleteRemoteBranchDialog";
import { DeleteRemoteTagDialog } from "./components/DeleteRemoteTagDialog";
import { HookFailureDialog } from "./components/HookFailureDialog";
import { MergeDialog } from "./components/MergeDialog";
import { ParentPickerDialog } from "./components/ParentPickerDialog";
import { ConfirmDialog } from "./components/primitives/ConfirmDialog";
import { Overlay } from "./components/primitives/Overlay";
import { PushBranchDialog } from "./components/PushBranchDialog";
import { RebasePlanner } from "./components/RebasePlanner";
import { RemoteDialogs } from "./components/RemoteDialogs";
import { ResetDialog } from "./components/ResetDialog";
import { RewordDialog } from "./components/RewordDialog";
import { ShortcutSheet } from "./components/ShortcutSheet";
import { StashDialog } from "./components/StashDialog";
import { TransferPanel } from "./components/TransferPanel";
import type { GraphCommit, RepoClient } from "./ipc/RepoClient";
import type { Command } from "./lib/commands";
import { retrySuccessLabel } from "./lib/hookFailure";
import { stashBlockReason, terminalAbortConfirm } from "./lib/operationStatus";
import { upstreamPushBranch } from "./lib/pushTarget";
import type { useAnnounce } from "./state/useAnnounce";
import type { UseAppStateResult } from "./state/useAppState";
import type { useCommitDraftActions } from "./state/useCommitDraftActions";
import type { useSequenceActions } from "./state/useSequenceActions";

/** The parent picker or reset dialog of one commit (#507, #514). */
export interface CommitDialogRequest {
  kind: "Cherry-pick" | "Revert" | "Reset";
  commit: GraphCommit;
  returnFocusTo: HTMLElement | null;
}

type SequenceActions = ReturnType<typeof useSequenceActions>;

/**
 * `RepoWorkspace`'s dialogs and overlays (#491). Each opens from its own state: mostly an
 * `appState.state.*Dialog` field, else one of the workspace's own (`commitDialog`, the palette,
 * the shortcut sheet).
 */
export function WorkspaceDialogs({
  active,
  repoPath,
  client,
  appState,
  announce,
  pushToast,
  operationDisabled: repositoryOperationDisabled,
  operationDisabledReason,
  runContinueSequence,
  retryWithCurrentMessage,
  terminalAbortRequest,
  onTerminalAbortRequestChange: setTerminalAbortRequest,
  runTerminalAbort,
  commitDialog,
  onCommitDialogChange: setCommitDialog,
  onCherryPick: cherryPickCommit,
  onRevert: revertCommit,
  onReset: resetToCommit,
  paletteOpen,
  onPaletteOpenChange: setPaletteOpen,
  paletteCommands,
  shortcutsOpen,
  onShortcutsOpenChange: setShortcutsOpen,
}: {
  // Only the active tab's dialogs show (see the `Overlay` note below).
  active: boolean;
  repoPath: string;
  client: RepoClient;
  appState: UseAppStateResult;
  announce: ReturnType<typeof useAnnounce>;
  pushToast: (message: string) => void;
  // The one "repository busy" rule and its reason (`operationBlockers`, #495).
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  runContinueSequence: SequenceActions["runContinueSequence"];
  retryWithCurrentMessage: ReturnType<typeof useCommitDraftActions>["retryWithCurrentMessage"];
  terminalAbortRequest: SequenceActions["terminalAbortRequest"];
  onTerminalAbortRequestChange: SequenceActions["setTerminalAbortRequest"];
  runTerminalAbort: SequenceActions["runTerminalAbort"];
  commitDialog: CommitDialogRequest | null;
  onCommitDialogChange: (request: CommitDialogRequest | null) => void;
  onCherryPick: (commitId: string, mainline: number | null) => void;
  onRevert: (commitId: string, mainline: number | null) => void;
  onReset: UseAppStateResult["resetToCommit"];
  paletteOpen: boolean;
  onPaletteOpenChange: (open: boolean) => void;
  // Built only while the palette is open.
  paletteCommands: () => Command[];
  shortcutsOpen: boolean;
  onShortcutsOpenChange: (open: boolean) => void;
}) {
  const { dismissError } = appState;
  const currentBranchName = appState.state.branches.find((branch) => branch.isCurrent)?.name ?? appState.state.upstream?.remoteBranch ?? "";
  const currentLocalBranch = appState.state.branches.find((branch) => branch.isCurrent)?.name ?? null;
  // #508: the stash dialog re-checks this while open; a rebase or sequence can start meanwhile.
  const stashBlocked = stashBlockReason(appState.state);
  // The branch `force_push_current_branch` overwrites on `remoteName`: what push.default resolves
  // for the upstream's remote, else the current branch's own name.
  const forcePushTargetBranch = (remoteName: string) =>
    (appState.state.upstream?.remoteName === remoteName ? upstreamPushBranch(appState.state.upstream) : null) ??
    currentBranchName;
  const pushDialogBranch = appState.state.pushDialog?.localBranch ?? null;
  const createTagDialog = appState.state.createTagDialog;
  const createTagCommit =
    createTagDialog === null ? undefined : appState.state.commits.find((commit) => commit.id === createTagDialog.commitId);
  const mergeDialogBranch = appState.state.mergeDialog?.branchName ?? null;
  const rewordDialog = appState.state.rewordDialog;
  const rewordCommitId = rewordDialog?.commitId ?? null;
  const loadRewordMessage = useCallback(
    () => (rewordCommitId === null ? Promise.resolve("") : client.getCommitMessage(repoPath, rewordCommitId)),
    [client, repoPath, rewordCommitId],
  );
  return (
    <>
      {active && appState.state.hookFailure?.dialogOpen === true && (
        <HookFailureDialog
          failure={appState.state.hookFailure}
          onRetry={() => {
            const retry = appState.state.hookFailure!.retry;
            // A Continue's toast comes from its outcome, like a plain Continue's. Dismissed first
            // (as every retry is), also when the sequence has gone meanwhile and nothing runs.
            if (retry.kind === "continueSequence") {
              dismissError();
              runContinueSequence(true);
            }
            // A pick's failure offers no retry (`retryLabel` is null), so this never runs for it.
            else if (retry.kind !== "pick") void announce(retrySuccessLabel(retry), retryWithCurrentMessage)(retry);
          }}
          onClose={appState.closeHookFailure}
        />
      )}
      {active && appState.state.forcePushConfirm !== null && (
        <ConfirmDialog
          ariaLabel="Confirm force push"
          message={
            <>
              <p>
                Overwrites {appState.state.forcePushConfirm.remoteName}/
                {forcePushTargetBranch(appState.state.forcePushConfirm.remoteName)} with your local branch.
              </p>
              {/* #514: the repository can become busy while the confirmation is open. */}
              {operationDisabledReason !== null && <p>{operationDisabledReason}</p>}
            </>
          }
          confirmLabel="Force push"
          confirmDisabled={repositoryOperationDisabled}
          confirmTitle={operationDisabledReason ?? undefined}
          onConfirm={() => {
            if (repositoryOperationDisabled) return;
            void announce("Force pushed", appState.forcePushCurrentBranch)(appState.state.forcePushConfirm!.remoteName);
          }}
          onCancel={appState.cancelForcePush}
        />
      )}
      {active && terminalAbortRequest !== null && (
        <ConfirmDialog
          ariaLabel={terminalAbortConfirm(terminalAbortRequest.sequence).title}
          message={
            <>
              <p>
                <strong>{terminalAbortConfirm(terminalAbortRequest.sequence).title}</strong>
              </p>
              <p>{terminalAbortConfirm(terminalAbortRequest.sequence).body}</p>
            </>
          }
          confirmLabel={terminalAbortConfirm(terminalAbortRequest.sequence).confirmLabel}
          onConfirm={() => {
            const request = terminalAbortRequest;
            setTerminalAbortRequest(null);
            runTerminalAbort(request.sequence, request.headId);
          }}
          onCancel={() => setTerminalAbortRequest(null)}
        />
      )}
      {active && mergeDialogBranch !== null && (
        <MergeDialog
          branchName={mergeDialogBranch}
          currentBranchName={currentBranchName}
          loadOptions={appState.getMergeOptions}
          blockedReason={operationDisabledReason}
          onMerge={(options) => {
            if (repositoryOperationDisabled) return;
            void appState.mergeBranch(mergeDialogBranch, options);
          }}
          onCancel={appState.closeMergeDialog}
        />
      )}
      {active && appState.state.stashDialog !== null && (
        <StashDialog
          paths={appState.state.stashDialog.paths}
          untrackedOnly={
            appState.state.stashDialog.paths.length > 0 &&
            appState.state.stashDialog.paths.every((path) =>
              appState.state.status.some((entry) => entry.path === path && !entry.staged && entry.kind === "New"),
            )
          }
          blockedReason={stashBlocked}
          onStash={(options) => {
            if (stashBlocked !== null) return;
            void announce("Changes stashed", appState.saveStash)(options);
          }}
          onCancel={appState.closeStashDialog}
        />
      )}
      {active && commitDialog !== null && commitDialog.kind !== "Reset" && (
        <ParentPickerDialog
          action={commitDialog.kind}
          commit={commitDialog.commit}
          commits={appState.state.commits}
          returnFocusTo={commitDialog.returnFocusTo}
          blockedReason={operationDisabledReason}
          onConfirm={(mainline) => {
            // Re-checked: the repository can become busy while the dialog is open (#514).
            if (repositoryOperationDisabled) return;
            setCommitDialog(null);
            if (commitDialog.kind === "Cherry-pick") cherryPickCommit(commitDialog.commit.id, mainline);
            else revertCommit(commitDialog.commit.id, mainline);
          }}
          onCancel={() => setCommitDialog(null)}
        />
      )}
      {active && commitDialog !== null && commitDialog.kind === "Reset" && currentLocalBranch !== null && (
        <ResetDialog
          branch={currentLocalBranch}
          shortId={commitDialog.commit.shortId}
          returnFocusTo={commitDialog.returnFocusTo}
          blockedReason={operationDisabledReason}
          onReset={(mode) => {
            if (repositoryOperationDisabled) return;
            setCommitDialog(null);
            void resetToCommit(commitDialog.commit.id, mode);
          }}
          onCancel={() => setCommitDialog(null)}
        />
      )}
      {active && pushDialogBranch !== null && (
        <PushBranchDialog
          localBranch={pushDialogBranch}
          remotes={appState.state.remotes}
          branchUpstream={appState.state.branches.find((branch) => branch.name === pushDialogBranch)?.upstream}
          blockedReason={operationDisabledReason}
          onPush={(remoteName, remoteBranch, setUpstream) => {
            if (repositoryOperationDisabled) return;
            void announce("Push complete", appState.pushBranch)(remoteName, pushDialogBranch, remoteBranch, setUpstream);
          }}
          onCancel={appState.closePushDialog}
        />
      )}
      {active && (
        // Here, not in `BranchTree`: the sidebar stays mounted but `hidden` while collapsed.
        <RemoteDialogs
          remoteDialog={appState.state.remoteDialog}
          remotes={appState.state.remotes}
          remoteUpstreams={appState.state.remoteUpstreams}
          blockedReason={operationDisabledReason}
          onRenameRemote={appState.renameRemote}
          onUpdateRemoteUrls={appState.updateRemoteUrls}
          onRemoveRemote={appState.removeRemote}
          onSaveHttpsCredential={appState.saveHttpsCredential}
          onForgetHttpsCredential={appState.forgetHttpsCredential}
          onSetRemoteAuthMode={appState.setRemoteAuthMode}
          onClose={appState.closeRemoteDialog}
          onRemoteGone={(name) => pushToast(`Remote ${name} was removed or renamed.`)}
        />
      )}
      {active && appState.state.deleteRemoteBranchDialog !== null && (
        <DeleteRemoteBranchDialog
          remotes={appState.state.remotes}
          initialRemote={appState.state.deleteRemoteBranchDialog.remoteName}
          initialBranch={appState.state.deleteRemoteBranchDialog.branchName}
          onListRemoteBranches={appState.listRemoteBranches}
          blockedReason={operationDisabledReason}
          onDelete={(remoteName, branchName) => {
            if (repositoryOperationDisabled) return;
            void announce("Remote branch deleted", appState.deleteRemoteBranch)(remoteName, branchName);
          }}
          onCancel={appState.closeDeleteRemoteBranchDialog}
        />
      )}
      {active && createTagDialog !== null && (
        <CreateTagDialog
          key={createTagDialog.commitId}
          shortId={createTagCommit?.shortId ?? createTagDialog.commitId.slice(0, 7)}
          summary={createTagCommit?.summary ?? null}
          existingTags={appState.state.tags.map((tag) => tag.name)}
          remotes={appState.state.remotes}
          disabled={repositoryOperationDisabled}
          disabledReason={operationDisabledReason}
          onCreate={async (name, message, pushTo) => {
            // With a push to follow, only the push is announced: its toast reports both steps.
            const create = pushTo === null ? announce("Tag created", appState.createTag) : appState.createTag;
            const failure = await create(name, message, createTagDialog.commitId);
            if (failure !== null) return failure;
            appState.closeCreateTagDialog();
            if (pushTo !== null) void announce("Push complete", appState.pushTags)(pushTo, [name]);
            return null;
          }}
          onCancel={appState.closeCreateTagDialog}
        />
      )}
      {active && rewordDialog !== null && (
        <RewordDialog
          key={rewordDialog.commitId}
          shortId={
            appState.state.commits.find((commit) => commit.id === rewordDialog.commitId)?.shortId ??
            rewordDialog.commitId.slice(0, 7)
          }
          loadMessage={loadRewordMessage}
          disabled={repositoryOperationDisabled}
          disabledReason={operationDisabledReason}
          onReword={(message) => {
            const { commitId, onto } = rewordDialog;
            appState.closeRewordDialog();
            void appState.editHistory(commitId, onto, { kind: "Reword", message });
          }}
          onCancel={appState.closeRewordDialog}
        />
      )}
      {active && appState.state.deleteRemoteTagDialog !== null && (
        <DeleteRemoteTagDialog
          tagName={appState.state.deleteRemoteTagDialog.tagName}
          remotes={appState.state.remotes}
          disabled={repositoryOperationDisabled}
          disabledReason={operationDisabledReason}
          onDelete={(remoteName, deleteLocal) =>
            void announce("Remote tag deleted", appState.deleteRemoteTag)(
              remoteName,
              appState.state.deleteRemoteTagDialog?.tagName ?? "",
              deleteLocal,
            )
          }
          onCancel={appState.closeDeleteRemoteTagDialog}
        />
      )}
      {/* Every `Overlay` below is gated on `active` as well as its own open-state. `Overlay`
          calls `dialog.showModal()`, which blocks the whole document (top-layer + `inert`
          outside the dialog) even when this wrapper is `display: none` — so a backgrounded tab
          whose transfer/palette/rebase overlay is open would freeze the visible tab behind an
          invisible modal. The transfer overlay makes that the common case: it opens by itself
          whenever `state.transfer !== null`, i.e. exactly the "start a push, switch tabs"
          flow this feature exists for. Gating unmounts the dialog (releasing the block) while
          backgrounded and loses nothing — `paletteOpen`/`transfer`/`rebaseOnto` all live in
          state this doesn't touch, so switching back re-shows the same overlay. */}
      {active && appState.state.transfer !== null && (
        <Overlay>
          <TransferPanel
            progress={appState.state.transfer}
            // Fire-and-forget: the backend records the cancel request and the transfer's own
            // terminal progress event (errorKind "Cancelled") is what closes this panel.
            onCancel={(operationId) => void client.cancelTransfer(repoPath, operationId)}
          />
        </Overlay>
      )}
      {active && paletteOpen && (
        <Overlay onClose={() => setPaletteOpen(false)}>
          <CommandPalette
            commands={paletteCommands()}
            onRun={() => setPaletteOpen(false)}
          />
        </Overlay>
      )}
      {active && shortcutsOpen && (
        <Overlay onClose={() => setShortcutsOpen(false)}>
          <ShortcutSheet onClose={() => setShortcutsOpen(false)} />
        </Overlay>
      )}
      {active && appState.state.rebaseOnto !== null && (
        <Overlay onClose={appState.closeRebasePlanner}>
          <RebasePlanner
            repoPath={repoPath}
            client={client}
            onto={appState.state.rebaseOnto}
            onStartRebase={(onto, plan) => {
              // Re-checked: the repository can become busy while the planner is open (#521).
              if (repositoryOperationDisabled) return;
              void appState.startRebase(onto, plan);
            }}
            onCancel={appState.closeRebasePlanner}
            blockedReason={operationDisabledReason}
            presetActions={appState.state.rebasePreset ?? undefined}
          />
        </Overlay>
      )}
    </>
  );
}
