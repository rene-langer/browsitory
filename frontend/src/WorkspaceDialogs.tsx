import { useCallback } from "react";
import { ApplyPatchDialog } from "./components/ApplyPatchDialog";
import { CommandPalette } from "./components/CommandPalette";
import { CreateTagDialog } from "./components/CreateTagDialog";
import { DeleteTagMessage } from "./components/DeleteTagMessage";
import { DeleteRemoteBranchDialog } from "./components/DeleteRemoteBranchDialog";
import { DeleteRemoteTagDialog } from "./components/DeleteRemoteTagDialog";
import { EditRestDialog } from "./components/EditRestDialog";
import { HookFailureDialog } from "./components/HookFailureDialog";
import { MergeDialog } from "./components/MergeDialog";
import { ParentPickerDialog } from "./components/ParentPickerDialog";
import { ConfirmDialog } from "./components/primitives/ConfirmDialog";
import { DropStashMessage } from "./components/DropStashMessage";
import { Overlay } from "./components/primitives/Overlay";
import { PendingPullPrompt } from "./components/PendingPullPrompt";
import { SwitchConflictDialog } from "./components/SwitchConflictDialog";
import { PushBranchDialog } from "./components/PushBranchDialog";
import { RebasePlanner } from "./components/RebasePlanner";
import { rebasePlannerTitle } from "./lib/rebasePlannerTitle";
import { RemoteDialogs } from "./components/RemoteDialogs";
import { ResetDialog } from "./components/ResetDialog";
import { RewordDialog } from "./components/RewordDialog";
import { ShortcutSheet } from "./components/ShortcutSheet";
import { StashDialog } from "./components/StashDialog";
import type { GraphCommit, RepoClient } from "./ipc/RepoClient";
import type { PushToast } from "./state/useToasts";
import type { Command } from "./lib/commands";
import { retrySuccessLabel } from "./lib/hookFailure";
import { busyReason, stashBlockReason, terminalAbortConfirm } from "./lib/operationStatus";
import { upstreamPushBranch } from "./lib/pushTarget";
import { repoNameOf } from "./lib/repoName";
import { shortRefName } from "./lib/shortRefName";
import { stashTitle } from "./lib/actions/stashActions";
import type { useAnnounce } from "./state/useAnnounce";
import type { UseAppStateResult } from "./state/useAppState";
import type { useCommitDraftActions } from "./state/useCommitDraftActions";
import type { useSequenceActions } from "./state/useSequenceActions";
import { dialogOf } from "./state/workspaceDialog";

/** The parent picker or reset dialog of one commit (#507, #514). */
export interface CommitDialogRequest {
  kind: "Cherry-pick" | "Revert" | "Reset";
  commit: GraphCommit;
  returnFocusTo: HTMLElement | null;
}

type SequenceActions = ReturnType<typeof useSequenceActions>;

/**
 * `RepoWorkspace`'s dialogs and overlays (#491). Each opens from its own state: mostly
 * `appState.state.dialog` (#493, one at a time), else one of the workspace's own (`commitDialog`,
 * the palette, the shortcut sheet) or another `AppState` field (hook failure, rebase
 * planner).
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
  abortRequest,
  onAbortRequestChange: setAbortRequest,
  onRebaseContinue,
  commitDialog,
  onCommitDialogChange: setCommitDialog,
  onCherryPick: cherryPickCommit,
  onRevert: revertCommit,
  onReset: resetToCommit,
  paletteOpen,
  onPaletteOpenChange: setPaletteOpen,
  paletteCommands,
  paletteInitialPicker,
  shortcutsOpen,
  onShortcutsOpenChange: setShortcutsOpen,
}: {
  // Only the active tab's dialogs show (see the `Overlay` note below).
  active: boolean;
  repoPath: string;
  client: RepoClient;
  appState: UseAppStateResult;
  announce: ReturnType<typeof useAnnounce>;
  pushToast: PushToast;
  // The one "repository busy" rule and its reason (`operationBlockers`, #495).
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  runContinueSequence: SequenceActions["runContinueSequence"];
  retryWithCurrentMessage: ReturnType<typeof useCommitDraftActions>["retryWithCurrentMessage"];
  terminalAbortRequest: SequenceActions["terminalAbortRequest"];
  onTerminalAbortRequestChange: SequenceActions["setTerminalAbortRequest"];
  runTerminalAbort: SequenceActions["runTerminalAbort"];
  // An Abort that would lose work, waiting for its confirmation (#979).
  abortRequest: SequenceActions["abortRequest"];
  onAbortRequestChange: SequenceActions["setAbortRequest"];
  // Continue rebase with its toast for a finished edit stop (#994).
  onRebaseContinue: SequenceActions["continueRebase"];
  commitDialog: CommitDialogRequest | null;
  onCommitDialogChange: (request: CommitDialogRequest | null) => void;
  onCherryPick: (commitId: string, mainline: number | null) => void;
  onRevert: (commitId: string, mainline: number | null) => void;
  onReset: UseAppStateResult["resetToCommit"];
  paletteOpen: boolean;
  onPaletteOpenChange: (open: boolean) => void;
  // Built only while the palette is open.
  paletteCommands: () => Command[];
  // The picker the palette opens into (the toolbar's branch chip), else `null`.
  paletteInitialPicker: string | null;
  shortcutsOpen: boolean;
  onShortcutsOpenChange: (open: boolean) => void;
}) {
  const { dismissError, closeDialog } = appState;
  const dialog = appState.state.dialog;
  const currentBranchName = appState.state.branches.find((branch) => branch.isCurrent)?.name ?? appState.state.upstream?.remoteBranch ?? "";
  const currentLocalBranch = appState.state.branches.find((branch) => branch.isCurrent)?.name ?? null;
  // #508: the stash dialog re-checks this while open; a rebase or sequence can start meanwhile.
  const stashBlocked = stashBlockReason(appState.state);
  // The branch `force_push_current_branch` overwrites on `remoteName`: what push.default resolves
  // for the upstream's remote, else the current branch's own name.
  const forcePushTargetBranch = (remoteName: string) =>
    (appState.state.upstream?.remoteName === remoteName ? upstreamPushBranch(appState.state.upstream) : null) ??
    currentBranchName;
  const forcePushConfirm = dialogOf(dialog, "forcePush");
  // What a force push to `remoteName` removes there (#989): the upstream's commits the branch
  // doesn't have, when the push goes to the upstream and the counts are known.
  const forcePushLostCount = (remoteName: string): number | null => {
    const upstream = appState.state.upstream;
    if (upstream === null || upstream.remoteName !== remoteName) return null;
    if (upstreamPushBranch(upstream) !== upstream.remoteBranch) return null;
    return upstream.behind ?? null;
  };
  // A diverged pull's commit counts (#1019), when the pull is the upstream's.
  const pendingPullUpstream = appState.state.upstream;
  const pendingPullCounts =
    appState.state.pendingPull !== null &&
    pendingPullUpstream !== null &&
    shortRefName(appState.state.pendingPull.upstreamRef) ===
      `${pendingPullUpstream.remoteName}/${pendingPullUpstream.remoteBranch}`
      ? { ahead: pendingPullUpstream.ahead ?? null, behind: pendingPullUpstream.behind ?? null }
      : null;
  const pushDialogBranch = dialogOf(dialog, "push")?.localBranch ?? null;
  const createTagDialog = dialogOf(dialog, "createTag");
  const createTagCommit =
    createTagDialog === null ? undefined : appState.state.commits.find((commit) => commit.id === createTagDialog.commitId);
  const mergeDialogBranch = dialogOf(dialog, "merge")?.branchName ?? null;
  const stashDialog = dialogOf(dialog, "stash");
  const applyPatchDialog = dialogOf(dialog, "applyPatch");
  const deleteRemoteBranchDialog = dialogOf(dialog, "deleteRemoteBranch");
  const deleteRemoteTagDialog = dialogOf(dialog, "deleteRemoteTag");
  const deleteTagDialog = dialogOf(dialog, "deleteTag");
  const dropStashDialog = dialogOf(dialog, "dropStash");
  // Found by commit, as the indexes shift when another stash is pushed or dropped meanwhile.
  const dropStashEntry =
    dropStashDialog === null ? undefined : appState.state.stashes.find((stash) => stash.commitId === dropStashDialog.commitId);
  const removeWorktreeDialog = dialogOf(dialog, "removeWorktree");
  const removeWorktreeEntry =
    removeWorktreeDialog === null
      ? undefined
      : appState.state.worktrees.find((worktree) => worktree.name === removeWorktreeDialog.name);
  const remoteDialog = dialogOf(dialog, "remote");
  const rewordDialog = dialogOf(dialog, "reword");
  const rewordShortId =
    rewordDialog === null
      ? ""
      : (appState.state.commits.find((commit) => commit.id === rewordDialog.commitId)?.shortId ??
        rewordDialog.commitId.slice(0, 7));
  // Read from `dialog` itself, not `rewordDialog`: the React Compiler cannot keep the memoization
  // below on a value a helper call returned.
  const rewordCommitId = dialog?.kind === "reword" ? dialog.commitId : null;
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
            // A push's toast comes from the transfer it starts (`transferToast`), like any push's.
            else if (retry.kind === "push") {
              dismissError();
              void retryWithCurrentMessage(retry);
            }
            // A pick's or patch's failure offers no retry (`retryLabel` is null), so this never
            // runs for them.
            else if (retry.kind !== "pick" && retry.kind !== "applyPatch") {
              void announce(retrySuccessLabel(retry), retryWithCurrentMessage, { undoable: true })(retry);
            }
          }}
          onClose={appState.closeHookFailure}
        />
      )}
      {active && forcePushConfirm !== null && (
        <ConfirmDialog
          title={`Force push to ${forcePushConfirm.remoteName}/${forcePushTargetBranch(forcePushConfirm.remoteName)}`}
          message={
            <ForcePushMessage
              target={`${forcePushConfirm.remoteName}/${forcePushTargetBranch(forcePushConfirm.remoteName)}`}
              branch={currentLocalBranch}
              lost={forcePushLostCount(forcePushConfirm.remoteName)}
            />
          }
          confirmLabel="Force push"
          tone="danger"
          confirmDisabled={repositoryOperationDisabled}
          // #514: the repository can become busy while the confirmation is open.
          confirmReason={operationDisabledReason}
          onConfirm={() => {
            if (repositoryOperationDisabled) return;
            void appState.forcePushCurrentBranch(forcePushConfirm.remoteName);
          }}
          onCancel={appState.cancelForcePush}
        />
      )}
      {active && terminalAbortRequest !== null && (
        <ConfirmDialog
          title={terminalAbortConfirm(terminalAbortRequest.sequence).title}
          message={<p>{terminalAbortConfirm(terminalAbortRequest.sequence).body}</p>}
          confirmLabel={terminalAbortConfirm(terminalAbortRequest.sequence).confirmLabel}
          tone="danger"
          onConfirm={() => {
            const request = terminalAbortRequest;
            setTerminalAbortRequest(null);
            runTerminalAbort(request.sequence, request.headId);
          }}
          onCancel={() => setTerminalAbortRequest(null)}
        />
      )}
      {active && abortRequest !== null && (
        // Abort that would lose work Undo can't bring back (#979); Cancel has the focus.
        <ConfirmDialog
          title={abortRequest.confirm.title}
          message={<p>{abortRequest.confirm.body}</p>}
          confirmLabel={abortRequest.confirm.confirmLabel}
          cancelLabel={abortRequest.confirm.cancelLabel}
          tone="danger"
          onConfirm={() => {
            const request = abortRequest;
            setAbortRequest(null);
            request.run();
          }}
          onCancel={() => setAbortRequest(null)}
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
          onCancel={() => closeDialog("merge")}
        />
      )}
      {active && stashDialog !== null && (
        <StashDialog
          paths={stashDialog.paths}
          untrackedOnly={
            stashDialog.paths.length > 0 &&
            stashDialog.paths.every((path) =>
              appState.state.status.some((entry) => entry.path === path && !entry.staged && entry.kind === "New"),
            )
          }
          blockedReason={stashBlocked}
          onStash={(options) => {
            if (stashBlocked !== null) return;
            void announce("Changes stashed", appState.saveStash)(options);
          }}
          onCancel={() => closeDialog("stash")}
        />
      )}
      {active && applyPatchDialog !== null && (
        <ApplyPatchDialog
          preview={applyPatchDialog.preview}
          hasStagedChanges={appState.state.status.some((entry) => entry.staged)}
          blockedReason={operationDisabledReason}
          onApply={(how) => {
            // Re-checked: the repository can become busy while the preview is open (#514).
            if (repositoryOperationDisabled) return;
            void appState.applyPatch(applyPatchDialog.content, applyPatchDialog.preview, how);
          }}
          onCancel={() => closeDialog("applyPatch")}
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
            void appState.pushBranch(remoteName, pushDialogBranch, remoteBranch, setUpstream);
          }}
          onCancel={() => closeDialog("push")}
        />
      )}
      {active && appState.state.switchConflict !== null && (
        <SwitchConflictDialog
          branchName={appState.state.switchConflict.branchName}
          currentBranch={currentLocalBranch}
          files={appState.state.switchConflict.files}
          operationDisabled={repositoryOperationDisabled}
          operationDisabledReason={operationDisabledReason}
          onStashAndSwitch={(reapply) => {
            if (repositoryOperationDisabled || appState.state.switchConflict === null) return;
            dismissError();
            void appState.stashAndSwitch(appState.state.switchConflict.branchName, reapply);
          }}
          onCancel={appState.closeSwitchConflict}
        />
      )}
      {active && appState.state.editRest !== null && (
        <EditRestDialog
          files={appState.state.editRest}
          operationDisabled={busyReason(appState.state) !== null}
          operationDisabledReason={busyReason(appState.state)}
          onChoose={(rest) => {
            // Allowed during the paused rebase it continues; only an action in flight blocks it.
            if (busyReason(appState.state) !== null) return;
            void onRebaseContinue(rest);
          }}
          onCancel={appState.closeEditRest}
        />
      )}
      {active && appState.state.pendingPull !== null && (
        // Here, not in `BranchTree`, for the same reason as `RemoteDialogs` below (#636).
        <PendingPullPrompt
          upstreamRef={appState.state.pendingPull.upstreamRef}
          noFastForward={appState.state.pendingPull.noFastForward === true}
          branchName={appState.state.branches.find((branch) => branch.isCurrent)?.name ?? null}
          repoName={repoNameOf(repoPath)}
          ahead={pendingPullCounts?.ahead ?? null}
          behind={pendingPullCounts?.behind ?? null}
          operationDisabled={repositoryOperationDisabled}
          operationDisabledReason={operationDisabledReason}
          getPullStrategy={appState.getPullStrategy}
          setPullStrategy={appState.setPullStrategy}
          onMerge={(upstreamRef, fastForward) => {
            appState.clearPendingPull();
            void appState.mergePull(upstreamRef, fastForward);
          }}
          onRebase={(upstreamRef) => {
            appState.clearPendingPull();
            void appState.rebasePull(upstreamRef);
          }}
          onCancel={appState.clearPendingPull}
        />
      )}
      {active && (
        // Here, not in `BranchTree`: the sidebar stays mounted but `hidden` while collapsed.
        <RemoteDialogs
          remoteDialog={remoteDialog}
          remotes={appState.state.remotes}
          remoteUpstreams={appState.state.remoteUpstreams}
          blockedReason={operationDisabledReason}
          onRenameRemote={appState.renameRemote}
          onUpdateRemoteUrls={appState.updateRemoteUrls}
          onRemoveRemote={appState.removeRemote}
          onSaveHttpsCredential={appState.saveHttpsCredential}
          onForgetHttpsCredential={appState.forgetHttpsCredential}
          onSetRemoteAuthMode={appState.setRemoteAuthMode}
          onClose={() => closeDialog("remote")}
          onRemoteGone={(name) => pushToast(`Remote ${name} was removed or renamed.`, { tone: "warning" })}
        />
      )}
      {active && deleteRemoteBranchDialog !== null && (
        <DeleteRemoteBranchDialog
          remotes={appState.state.remotes}
          initialRemote={deleteRemoteBranchDialog.remoteName}
          initialBranch={deleteRemoteBranchDialog.branchName}
          onListRemoteBranches={appState.listRemoteBranches}
          blockedReason={operationDisabledReason}
          onDelete={(remoteName, branchName) => {
            if (repositoryOperationDisabled) return;
            void appState.deleteRemoteBranch(remoteName, branchName);
          }}
          onCancel={() => closeDialog("deleteRemoteBranch")}
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
            const create = pushTo === null ? announce(`Created tag ${name}`, appState.createTag, { undoable: true }) : appState.createTag;
            const failure = await create(name, message, createTagDialog.commitId);
            if (failure !== null) return failure;
            closeDialog("createTag");
            if (pushTo !== null) void appState.pushTags(pushTo, [name]);
            return null;
          }}
          onCancel={() => closeDialog("createTag")}
        />
      )}
      {active && rewordDialog !== null && (
        <RewordDialog
          key={rewordDialog.commitId}
          shortId={rewordShortId}
          head={rewordDialog.onto === null}
          loadMessage={loadRewordMessage}
          disabled={repositoryOperationDisabled}
          disabledReason={operationDisabledReason}
          onReword={(message) => {
            const { commitId, onto } = rewordDialog;
            closeDialog("reword");
            if (onto !== null) {
              void appState.editHistory(commitId, onto, { kind: "Reword", message });
              return;
            }
            // HEAD (#981): its message only, staged changes stay staged; one undo step.
            void announce(`Reworded ${rewordShortId}`, appState.amendCommit, { undoable: true })(message, false, {
              author: null,
              trailers: [],
              messageOnly: true,
            });
          }}
          onCancel={() => closeDialog("reword")}
        />
      )}
      {active && deleteTagDialog !== null && (
        // The palette's "Delete tag …" (#993): TagPanel's confirmation.
        <ConfirmDialog
          title={`Delete local tag ${deleteTagDialog.tagName}`}
          message={<DeleteTagMessage name={deleteTagDialog.tagName} />}
          confirmLabel="Delete tag"
          confirmDisabled={repositoryOperationDisabled}
          // The repository can become busy while the confirmation is open (#521).
          confirmReason={operationDisabledReason}
          onConfirm={() => {
            if (repositoryOperationDisabled) return;
            void appState.deleteTag(deleteTagDialog.tagName).then(() => closeDialog("deleteTag"));
          }}
          onCancel={() => closeDialog("deleteTag")}
        />
      )}
      {active && dropStashDialog !== null && dropStashEntry !== undefined && (
        // The palette's "Drop stash: …" (#990): the Stashes panel's confirmation. Only an action in
        // flight or a transfer blocks a drop (#521).
        <ConfirmDialog
          title={`Drop stash ${stashTitle(dropStashEntry)}`}
          message={<DropStashMessage />}
          confirmLabel="Drop stash"
          tone="danger"
          confirmReason={busyReason(appState.state)}
          onConfirm={() => {
            if (busyReason(appState.state) !== null) return;
            closeDialog("dropStash");
            void appState.dropStash(dropStashEntry.index);
          }}
          onCancel={() => closeDialog("dropStash")}
        />
      )}
      {active && removeWorktreeDialog !== null && removeWorktreeEntry !== undefined && (
        // The palette's "Remove worktree …" (#990): the Worktrees panel's confirmation.
        <ConfirmDialog
          title={`Remove worktree ${removeWorktreeEntry.path}`}
          message={<p>Remove worktree at {removeWorktreeEntry.path}?</p>}
          confirmLabel="Remove worktree"
          confirmDisabled={repositoryOperationDisabled}
          confirmReason={operationDisabledReason}
          onConfirm={() => {
            if (repositoryOperationDisabled) return;
            void appState.removeWorktree(removeWorktreeEntry.name).then(() => closeDialog("removeWorktree"));
          }}
          onCancel={() => closeDialog("removeWorktree")}
        />
      )}
      {active && deleteRemoteTagDialog !== null && (
        <DeleteRemoteTagDialog
          tagName={deleteRemoteTagDialog.tagName}
          remotes={appState.state.remotes}
          disabled={repositoryOperationDisabled}
          disabledReason={operationDisabledReason}
          onDelete={(remoteName, deleteLocal) =>
            void appState.deleteRemoteTag(
              remoteName,
              deleteRemoteTagDialog.tagName,
              deleteLocal,
            )
          }
          onCancel={() => closeDialog("deleteRemoteTag")}
        />
      )}
      {/* Every `Overlay` below is gated on `active` as well as its own open-state. `Overlay`
          calls `dialog.showModal()`, which blocks the whole document (top-layer + `inert`
          outside the dialog) even when this wrapper is `display: none` — so a backgrounded tab
          whose palette/rebase overlay is open would freeze the visible tab behind an invisible
          modal. Gating unmounts the dialog (releasing the block) while backgrounded and loses
          nothing — `paletteOpen`/`rebaseOnto` live in state this doesn't touch, so switching
          back re-shows the same overlay. A running fetch, pull or push is not a dialog: the
          toolbar shows it, with Cancel (`RepoActivity`, #659). */}
      {active && paletteOpen && (
        <Overlay onClose={() => setPaletteOpen(false)} ariaLabel="Command palette">
          <CommandPalette
            commands={paletteCommands()}
            initialPickerId={paletteInitialPicker}
            onRun={() => setPaletteOpen(false)}
          />
        </Overlay>
      )}
      {active && shortcutsOpen && (
        <Overlay onClose={() => setShortcutsOpen(false)} ariaLabel="Keyboard shortcuts">
          <ShortcutSheet onClose={() => setShortcutsOpen(false)} />
        </Overlay>
      )}
      {active && appState.state.rebaseOnto !== null && (
        <Overlay onClose={appState.closeRebasePlanner} ariaLabel={rebasePlannerTitle(appState.state.rebaseOnto, currentLocalBranch)}>
          <RebasePlanner
            repoPath={repoPath}
            client={client}
            onto={appState.state.rebaseOnto}
            branchName={currentLocalBranch}
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

/**
 * What a force push does (#989): what it replaces, that `--force-with-lease` refuses it when the
 * remote branch moved since the last fetch, which remote commits it removes, and that Undo can't
 * take a push back.
 */
function ForcePushMessage({ target, branch, lost }: { target: string; branch: string | null; lost: number | null }) {
  return (
    <>
      <p>
        Replaces {target} with your local {branch ?? "branch"}. It uses <code>--force-with-lease</code>: if {target}{" "}
        changed since your last fetch, the push is refused.
      </p>
      <p>
        {lost === null
          ? `Commits on ${target} that your branch doesn't have are removed from it.`
          : lost === 0
            ? `${target} has no commits your branch doesn't have.`
            : lost === 1
              ? `The 1 commit on ${target} that your branch doesn't have is removed from it.`
              : `The ${lost} commits on ${target} that your branch doesn't have are removed from it.`}
      </p>
      <p>Undo can&apos;t take back a push.</p>
    </>
  );
}
