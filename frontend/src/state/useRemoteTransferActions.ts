import { useCallback, useEffect, useRef } from "react";
import type { PullOutcome, RemoteAuthMode, RepoClient, TransferProgress } from "../ipc/RepoClient";
import { writeFetchPrune } from "../lib/fetchPrune";
import { upstreamPushBranch } from "../lib/pushTarget";
import { credentialFailureMessage } from "./useMutationRunner";
import type {
  RunMutation,
  RunMutationWithMessage,
  RunMutationWithOutcome,
  RunOptimisticMutation,
  RunOptimisticMutationWithMessage,
  RunOptimisticMutationWithOutcome,
} from "./useMutationRunner";
import type { UpstreamInfo } from "../ipc/RepoClient";
import type { AppState, RemoteDialog } from "./useAppState";
import { hookFailedMessage, type PushRequest } from "../lib/hookFailure";

function transferFailureMessage(progress: TransferProgress, upstreamLabel: string): string {
  // A cancellation reaches the UI as a `Failed` terminal event — the transfer really did stop
  // short — but the user pressed Cancel themselves, so it must not read as "Fetch failed".
  // Reporting it here (rather than treating it as a completion) also keeps `state.error`
  // non-null, which is what stops `App`'s success toast from announcing "Fetch complete" for a
  // transfer that never completed.
  if (progress.errorKind === "Cancelled") {
    if (progress.operation === "Pull") return "Pull cancelled.";
    if (progress.operation === "Fetch" || progress.operation === "FetchAll") return "Fetch cancelled.";
    if (progress.operation === "DeleteRemoteBranch") return "Delete cancelled.";
    return "Push cancelled.";
  }
  if (progress.errorKind === "HookFailed") return hookFailedMessage("pre-push");
  if (progress.errorKind === "MissingCredential") return credentialFailureMessage("missing credential");
  if (progress.errorKind === "CredentialStoreFailure") return credentialFailureMessage("credential keychain failure");
  if (progress.errorKind === "SshAgentFailure") return credentialFailureMessage("SSH agent failure");
  if (progress.errorKind === "StaleLease") {
    return `${upstreamLabel} changed since your last fetch. Fetch and review before force pushing.`;
  }
  if (progress.operation === "DeleteRemoteBranch") {
    return progress.errorKind === "RejectedRemoteRef"
      ? "The remote refused to delete the branch."
      : "Deleting the remote branch failed";
  }
  const isPush =
    progress.operation === "PushBranch" || progress.operation === "PushTags" || progress.operation === "ForcePushBranch";
  if (progress.operation === "ForcePushBranch" && progress.errorKind === "NonFastForward") {
    return "The remote refused the force push.";
  }
  if (isPush && progress.errorKind === "NonFastForward") {
    return "Push was rejected because the remote has newer commits. Pull or reconcile history, then try again.";
  }
  if (isPush && progress.errorKind === "RejectedRemoteRef") {
    return "The remote rejected the pushed reference.";
  }
  if (isPush) return "Push failed";
  if (progress.operation === "Pull") return "Pull failed";
  if (progress.operation === "Fetch" || progress.operation === "FetchAll") return "Fetch failed";
  return "Transfer failed";
}

/** One remote that failed during a Fetch all, as its `RemoteFailed` event reported it. */
interface FailedRemote {
  remoteName: string;
  errorKind: TransferProgress["errorKind"];
}

// The same fixed hints `transferFailureMessage` gives a single-remote transfer.
function credentialHint(errorKind: TransferProgress["errorKind"]): string | null {
  if (errorKind === "MissingCredential") return credentialFailureMessage("missing credential");
  if (errorKind === "CredentialStoreFailure") return credentialFailureMessage("credential keychain failure");
  if (errorKind === "SshAgentFailure") return credentialFailureMessage("SSH agent failure");
  return null;
}

function fetchAllFailureMessage(failed: FailedRemote[], othersFetched: boolean): string {
  const parts = [`Could not fetch from ${failed.map((remote) => remote.remoteName).join(", ")}.`];
  if (othersFetched) parts.push("The other remotes were fetched.");
  for (const remote of failed) {
    const hint = credentialHint(remote.errorKind);
    if (hint !== null) parts.push(`${remote.remoteName}: ${hint}`);
  }
  return parts.join(" ");
}

export interface RemoteTransferActions {
  // Resolves to `null` on success, or the failure message on failure — `RemotePanel` renders
  // that message inline next to the Fetch URL field and keeps the typed values. A plain
  // `runMutation` can't serve that: it swallows the error into `state.error` and *always*
  // resolves, so a `try`/`catch` around this call in the panel would be dead code and its
  // success path would wipe the user's input on a failed add. Returning the message (rather
  // than `renameRemote`'s `boolean`) keeps the panel off global `state.error`, which isn't
  // scoped to this action and can hold a stale message from an unrelated mutation.
  addRemote(name: string, fetchUrl: string, pushUrl: string | null): Promise<string | null>;
  renameRemote(oldName: string, newName: string): Promise<boolean>;
  updateRemoteUrls(name: string, fetchUrl: string, pushUrl: string | null): Promise<void>;
  removeRemote(name: string, clearUpstreams: boolean): Promise<void>;
  saveHttpsCredential(remoteName: string, username: string, token: string): Promise<void>;
  forgetHttpsCredential(remoteName: string): Promise<void>;
  setRemoteAuthMode(remoteName: string, mode: RemoteAuthMode, username: string | null): Promise<boolean>;
  setCurrentUpstream(remoteName: string, remoteBranch: string): Promise<void>;
  clearCurrentUpstream(): Promise<void>;
  listRemoteBranches(remoteName: string): Promise<string[]>;
  fetchRemote(remoteName: string): Promise<void>;
  // `Promise<string | null>` for the same reason as `addRemote` above.
  createTag(name: string, message: string | null): Promise<string | null>;
  deleteTag(name: string): Promise<void>;
  // `noVerify` skips the `pre-push` hook (the hook-failure dialog's retry); default `false`.
  pushCurrentBranch(remoteName: string, noVerify?: boolean): Promise<void>;
  pushTags(remoteName: string, names: string[], noVerify?: boolean): Promise<void>;
  pullCurrentUpstream(): Promise<void>;
  clearPendingPull(): void;
  openAddRemoteDraft(): void;
  closeAddRemoteDraft(): void;
  // Opens a remote's Edit / Credentials / Remove dialog (BranchTree renders it); no client call.
  openRemoteDialog(kind: RemoteDialog["kind"], remoteName: string): void;
  closeRemoteDialog(): void;
  requestForcePush(remoteName: string): void;
  cancelForcePush(): void;
  forcePushCurrentBranch(remoteName: string, noVerify?: boolean): Promise<void>;
  fetchAllRemotes(): Promise<void>;
  // Persists the Prune choice for this repo; every later fetch passes it explicitly.
  setFetchPrune(prune: boolean): void;
  pushBranch(remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean, noVerify?: boolean): Promise<void>;
  openPushDialog(localBranch: string): void;
  closePushDialog(): void;
  deleteRemoteBranch(remoteName: string, branchName: string, noVerify?: boolean): Promise<void>;
  // Both null opens the palette's picker mode; both set is a plain confirmation.
  openDeleteRemoteBranchDialog(remoteName: string | null, branchName: string | null): void;
  closeDeleteRemoteBranchDialog(): void;
}

// Owns everything transfer-progress-shaped (fetch/push/pull), plus the rest of remote/upstream/tag
// management that shares its `startTransfer`/credential-failure plumbing — the "remotes/transfer"
// domain named in MAINT-002. Also owns the `subscribeTransferProgress` effect and the
// `activeTransferId`/`transferRequestPending` refs that coordinate it with `startTransfer` and
// `pullCurrentUpstream`, since nothing outside this domain touches transfer progress.
export function useRemoteTransferActions(
  client: RepoClient,
  repoPath: string,
  refresh: () => Promise<void>,
  runMutation: RunMutation,
  _runMutationWithMessage: RunMutationWithMessage,
  runMutationWithOutcome: RunMutationWithOutcome,
  runOptimisticMutation: RunOptimisticMutation,
  runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage,
  runOptimisticMutationWithOutcome: RunOptimisticMutationWithOutcome,
  setState: (updater: (prev: AppState) => AppState) => void,
  upstream: UpstreamInfo | null,
  fetchPrune: boolean | null,
  currentBranch: string | null,
): RemoteTransferActions {
  const activeTransferId = useRef<string | null>(null);
  const transferRequestPending = useRef(false);
  const lastPushRemote = useRef<string | null>(null);
  // The last push-shaped request started, so a `pre-push` failure can be retried without hooks.
  const lastPushRequest = useRef<PushRequest | null>(null);
  // The `<remote>/<branch>` a force push targets (the lease and confirm dialog use the force-push
  // remote and the local branch name), recorded when it starts so a stale-lease failure can name it.
  const forcePushTarget = useRef<{ remoteName: string; branch: string | null } | null>(null);
  const upstreamRef = useRef<UpstreamInfo | null>(upstream);
  useEffect(() => {
    upstreamRef.current = upstream;
  }, [upstream]);
  const fetchPruneRef = useRef<boolean | null>(fetchPrune);
  useEffect(() => {
    fetchPruneRef.current = fetchPrune;
  }, [fetchPrune]);
  const currentBranchRef = useRef<string | null>(currentBranch);
  useEffect(() => {
    currentBranchRef.current = currentBranch;
  }, [currentBranch]);
  // Remotes a Fetch all reported as failed (`RemoteFailed`), for the active operation only.
  const failedRemotes = useRef<FailedRemote[]>([]);

  useEffect(() => {
    return client.subscribeTransferProgress((progress) => {
      if (progress.phase === "Starting") {
        if (!transferRequestPending.current || activeTransferId.current !== null) return;
        activeTransferId.current = progress.operationId;
        failedRemotes.current = [];
        setState((prev) => ({ ...prev, transfer: progress }));
        return;
      }

      if (progress.operationId !== activeTransferId.current) return;

      if (progress.phase === "RemoteFailed") {
        if (progress.remoteName !== undefined) {
          failedRemotes.current = [
            ...failedRemotes.current,
            { remoteName: progress.remoteName, errorKind: progress.errorKind },
          ];
        }
        return;
      }

      if (progress.phase === "Completed") {
        transferRequestPending.current = false;
        const failed = failedRemotes.current;
        failedRemotes.current = [];
        setState((prev) => ({ ...prev, forcePushOffer: null }));
        void refresh().finally(() => {
          if (activeTransferId.current !== progress.operationId) return;
          activeTransferId.current = null;
          // Set after `refresh()` (which resets `error`) and in the same update that clears
          // `transfer`, so a partial Fetch all both reports the dead remotes and suppresses
          // App's "Fetch complete" toast.
          setState((prev) => ({
            ...prev,
            transfer: null,
            pending: false,
            error: failed.length > 0 ? fetchAllFailureMessage(failed, true) : prev.error,
          }));
        });
        return;
      }

      if (progress.phase === "Failed") {
        transferRequestPending.current = false;
        activeTransferId.current = null;
        const current = upstreamRef.current;
        const target = progress.operation === "ForcePushBranch" ? forcePushTarget.current : null;
        const upstreamLabel =
          target !== null && target.branch !== null
            ? `${target.remoteName}/${target.branch}`
            : current !== null
              ? `${current.remoteName}/${current.remoteBranch}`
              : (lastPushRemote.current ?? "The remote branch");
        const offer =
          progress.operation === "PushBranch" && progress.errorKind === "NonFastForward" && lastPushRemote.current !== null
            ? { remoteName: lastPushRemote.current }
            : null;
        const failed = failedRemotes.current;
        failedRemotes.current = [];
        const message =
          progress.operation === "FetchAll" && progress.errorKind !== "Cancelled" && failed.length > 0
            ? fetchAllFailureMessage(failed, false)
            : transferFailureMessage(progress, upstreamLabel);
        const push = lastPushRequest.current;
        setState((prev) => ({
          ...prev,
          transfer: null,
          error: message,
          pending: false,
          forcePushOffer: offer,
          hookFailure:
            progress.errorKind === "HookFailed" && push !== null
              ? { hook: "pre-push", output: progress.hookOutput ?? "", retry: { kind: "push", push }, dialogOpen: true }
              : null,
        }));
        return;
      }

      setState((prev) => ({ ...prev, transfer: progress }));
    });
  }, [client, refresh, repoPath, setState]);

  const addRemote = useCallback(
    (name: string, fetchUrl: string, pushUrl: string | null) =>
      runOptimisticMutationWithMessage(
        (prev) => ({
          ...prev,
          remotes: [...prev.remotes, { name, fetchUrl, pushUrl, authMode: null, authUsername: null }],
        }),
        () => client.addRemote(repoPath, name, fetchUrl, pushUrl),
      ),
    [client, runOptimisticMutationWithMessage, repoPath],
  );
  const renameRemote = useCallback(
    (oldName: string, newName: string) =>
      runOptimisticMutationWithOutcome(
        (prev) => ({
          ...prev,
          remotes: prev.remotes.map((remote) => (remote.name === oldName ? { ...remote, name: newName } : remote)),
        }),
        () => client.renameRemote(repoPath, oldName, newName),
      ),
    [client, runOptimisticMutationWithOutcome, repoPath],
  );
  const updateRemoteUrls = useCallback(
    (name: string, fetchUrl: string, pushUrl: string | null) =>
      runOptimisticMutation(
        (prev) => ({
          ...prev,
          remotes: prev.remotes.map((remote) => (remote.name === name ? { ...remote, fetchUrl, pushUrl } : remote)),
        }),
        () => client.updateRemoteUrls(repoPath, name, fetchUrl, pushUrl),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const removeRemote = useCallback(
    (name: string, clearUpstreams: boolean) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, remotes: prev.remotes.filter((remote) => remote.name !== name) }),
        () => client.removeRemote(repoPath, name, clearUpstreams),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const saveHttpsCredential = useCallback(
    (remoteName: string, username: string, token: string) =>
      runMutation(() => client.saveHttpsCredential(repoPath, remoteName, username, token)),
    [client, runMutation, repoPath],
  );
  const forgetHttpsCredential = useCallback(
    (remoteName: string) => runMutation(() => client.forgetHttpsCredential(repoPath, remoteName)),
    [client, runMutation, repoPath],
  );
  const setRemoteAuthMode = useCallback(
    (remoteName: string, mode: RemoteAuthMode, username: string | null) =>
      runMutationWithOutcome(() => client.setRemoteAuthMode(repoPath, remoteName, mode, username)),
    [client, runMutationWithOutcome, repoPath],
  );
  const setCurrentUpstream = useCallback(
    (remoteName: string, remoteBranch: string) =>
      runOptimisticMutation(
        (prev) => {
          const localBranch = prev.branches.find((branch) => branch.isCurrent)?.name;
          if (localBranch === undefined) return prev;
          return { ...prev, upstream: { localBranch, remoteName, remoteBranch }, pullOutcome: null };
        },
        () => client.setCurrentUpstream(repoPath, remoteName, remoteBranch),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const clearCurrentUpstream = useCallback(
    () =>
      runOptimisticMutation(
        (prev) => ({ ...prev, upstream: null, pullOutcome: null }),
        () => client.clearCurrentUpstream(repoPath),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const startTransfer = useCallback(
    async (operation: TransferProgress["operation"], start: () => Promise<string>) => {
      try {
        transferRequestPending.current = true;
        activeTransferId.current = null;
        failedRemotes.current = [];
        setState((prev) => ({ ...prev, pending: true, error: null, forcePushOffer: null, hookFailure: null }));
        const operationId = await start();
        if (transferRequestPending.current && activeTransferId.current === null) {
          activeTransferId.current = operationId;
          setState((prev) => ({
            ...prev,
            transfer: {
              operationId,
              operation,
              phase: "Starting",
              errorKind: null,
              current: 0,
              total: 0,
              receivedBytes: 0,
              message: null,
              hookOutput: null,
            },
          }));
        }
      } catch (err) {
        const message = String(err);
        if (operation === "Fetch" && (message === "Fetch failed" || message.endsWith(": Fetch failed"))) return;
        transferRequestPending.current = false;
        activeTransferId.current = null;
        setState((prev) => ({ ...prev, error: message, pending: false }));
      }
    },
    [setState],
  );

  const fetchRemote = useCallback(
    (remoteName: string) => {
      lastPushRequest.current = null;
      return startTransfer("Fetch", () => client.fetchRemote(repoPath, remoteName, fetchPruneRef.current));
    },
    [client, startTransfer, repoPath],
  );
  const fetchAllRemotes = useCallback(() => {
    lastPushRequest.current = null;
    return startTransfer("FetchAll", () => client.fetchAllRemotes(repoPath, fetchPruneRef.current));
  }, [client, startTransfer, repoPath]);
  const setFetchPrune = useCallback(
    (prune: boolean) => {
      writeFetchPrune(repoPath, prune);
      fetchPruneRef.current = prune;
      setState((prev) => ({ ...prev, fetchPrune: prune }));
    },
    [repoPath, setState],
  );
  const listRemoteBranches = useCallback(
    (remoteName: string) => client.listRemoteBranches(repoPath, remoteName),
    [client, repoPath],
  );

  const createTag = useCallback(
    (name: string, message: string | null) =>
      runOptimisticMutationWithMessage(
        (prev) => ({
          ...prev,
          tags: [
            ...prev.tags,
            { name, targetId: "", annotated: message !== null, message, taggerName: null, timestamp: null },
          ],
        }),
        () => client.createTag(repoPath, name, message),
      ),
    [client, runOptimisticMutationWithMessage, repoPath],
  );
  const deleteTag = useCallback(
    (name: string) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, tags: prev.tags.filter((tag) => tag.name !== name) }),
        () => client.deleteTag(repoPath, name),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const pushCurrentBranch = useCallback(
    async (remoteName: string, noVerify = false) => {
      // push.default does not choose where the current branch goes on its upstream's remote
      // (e.g. `simple` with a differently named upstream): ask with Push to… (prefilled with
      // that remote and the local name) instead of pushing. The backend refuses such a push too.
      const upstream = upstreamRef.current;
      if (upstream !== null && upstream.remoteName === remoteName && upstreamPushBranch(upstream) === null) {
        const localBranch = currentBranchRef.current ?? upstream.localBranch;
        setState((prev) => ({ ...prev, pushDialog: { localBranch } }));
        return;
      }
      lastPushRemote.current = remoteName;
      lastPushRequest.current = { operation: "PushBranch", remoteName };
      return startTransfer("PushBranch", () => client.pushCurrentBranch(repoPath, remoteName, noVerify));
    },
    [client, startTransfer, repoPath, setState],
  );
  const requestForcePush = useCallback(
    (remoteName: string) => setState((prev) => ({ ...prev, forcePushConfirm: { remoteName } })),
    [setState],
  );
  const cancelForcePush = useCallback(
    () => setState((prev) => ({ ...prev, forcePushOffer: null, forcePushConfirm: null })),
    [setState],
  );
  const forcePushCurrentBranch = useCallback(
    (remoteName: string, noVerify = false) => {
      // The lease targets what `force_push_current_branch` resolves under push.default (see
      // `lib/pushTarget.ts`), else the current branch's own name.
      const upstream = upstreamRef.current;
      const leased = upstream !== null && upstream.remoteName === remoteName ? upstreamPushBranch(upstream) : null;
      forcePushTarget.current = { remoteName, branch: leased ?? upstream?.localBranch ?? currentBranchRef.current };
      setState((prev) => {
        const branch = prev.branches.find((candidate) => candidate.isCurrent)?.name;
        if (leased === null && branch !== undefined) forcePushTarget.current = { remoteName, branch };
        return { ...prev, forcePushOffer: null, forcePushConfirm: null };
      });
      lastPushRequest.current = { operation: "ForcePushBranch", remoteName };
      return startTransfer("ForcePushBranch", () => client.forcePushCurrentBranch(repoPath, remoteName, noVerify));
    },
    [client, startTransfer, repoPath, setState],
  );
  const pushTags = useCallback(
    (remoteName: string, names: string[], noVerify = false) => {
      lastPushRequest.current = { operation: "PushTags", remoteName, names };
      return startTransfer("PushTags", () => client.pushTags(repoPath, remoteName, names, noVerify));
    },
    [client, startTransfer, repoPath],
  );
  const pushBranch = useCallback(
    (remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean, noVerify = false) => {
      // The force-push offer force-pushes the *current* branch to where force_push_current_branch
      // resolves it: only offer it when that is exactly what this push targeted.
      const upstream = upstreamRef.current;
      const forceTarget =
        upstream !== null && upstream.remoteName === remoteName ? upstreamPushBranch(upstream) : localBranch;
      lastPushRemote.current =
        forceTarget !== null && localBranch === currentBranchRef.current && remoteBranch === forceTarget
          ? remoteName
          : null;
      setState((prev) => ({ ...prev, pushDialog: null }));
      lastPushRequest.current = { operation: "PushBranchTo", remoteName, localBranch, remoteBranch, setUpstream };
      return startTransfer("PushBranch", () =>
        client.pushBranch(repoPath, remoteName, localBranch, remoteBranch, setUpstream, noVerify),
      );
    },
    [client, startTransfer, repoPath, setState],
  );
  const deleteRemoteBranch = useCallback(
    (remoteName: string, branchName: string, noVerify = false) => {
      setState((prev) => ({ ...prev, deleteRemoteBranchDialog: null }));
      lastPushRequest.current = { operation: "DeleteRemoteBranch", remoteName, branchName };
      return startTransfer("DeleteRemoteBranch", () =>
        client.deleteRemoteBranch(repoPath, remoteName, branchName, noVerify),
      );
    },
    [client, startTransfer, repoPath, setState],
  );
  const openPushDialog = useCallback(
    (localBranch: string) => setState((prev) => ({ ...prev, pushDialog: { localBranch } })),
    [setState],
  );
  const closePushDialog = useCallback(() => setState((prev) => ({ ...prev, pushDialog: null })), [setState]);
  const openDeleteRemoteBranchDialog = useCallback(
    (remoteName: string | null, branchName: string | null) =>
      setState((prev) => ({ ...prev, deleteRemoteBranchDialog: { remoteName, branchName } })),
    [setState],
  );
  const closeDeleteRemoteBranchDialog = useCallback(
    () => setState((prev) => ({ ...prev, deleteRemoteBranchDialog: null })),
    [setState],
  );

  const pullCurrentUpstream = useCallback(async () => {
    try {
      transferRequestPending.current = true;
      activeTransferId.current = null;
      setState((prev) => ({
        ...prev,
        pending: true,
        error: null,
        pendingPull: null,
        pullOutcome: null,
      }));
      const outcome: PullOutcome = await client.pullCurrentUpstream(repoPath);
      if (outcome.kind === "Diverged") {
        setState((prev) => ({ ...prev, pending: false, pendingPull: { upstreamRef: outcome.upstreamRef } }));
        return;
      }
      await refresh();
      setState((prev) => ({ ...prev, pending: false, pullOutcome: outcome }));
    } catch (err) {
      const message = String(err);
      if (message === "pull failed" || message.endsWith(": pull failed")) return;
      transferRequestPending.current = false;
      activeTransferId.current = null;
      setState((prev) => ({
        ...prev,
        transfer: null,
        pending: false,
        pendingPull: null,
        pullOutcome: null,
        error: message.includes("cannot pull with a dirty worktree")
          ? "Commit or stash your changes before pulling."
          : message,
      }));
    }
  }, [client, refresh, repoPath, setState]);
  const clearPendingPull = useCallback(() => {
    setState((prev) => ({ ...prev, pendingPull: null }));
  }, [setState]);

  const openAddRemoteDraft = useCallback(() => {
    setState((prev) => ({ ...prev, addRemoteDraftOpen: true }));
  }, [setState]);
  const closeAddRemoteDraft = useCallback(() => {
    setState((prev) => ({ ...prev, addRemoteDraftOpen: false }));
  }, [setState]);
  const openRemoteDialog = useCallback(
    (kind: RemoteDialog["kind"], remoteName: string) =>
      setState((prev) => ({ ...prev, remoteDialog: { kind, remoteName } })),
    [setState],
  );
  const closeRemoteDialog = useCallback(() => setState((prev) => ({ ...prev, remoteDialog: null })), [setState]);

  return {
    addRemote,
    renameRemote,
    updateRemoteUrls,
    removeRemote,
    saveHttpsCredential,
    forgetHttpsCredential,
    setRemoteAuthMode,
    setCurrentUpstream,
    clearCurrentUpstream,
    listRemoteBranches,
    fetchRemote,
    createTag,
    deleteTag,
    pushCurrentBranch,
    pushTags,
    pullCurrentUpstream,
    clearPendingPull,
    openAddRemoteDraft,
    closeAddRemoteDraft,
    openRemoteDialog,
    closeRemoteDialog,
    requestForcePush,
    cancelForcePush,
    forcePushCurrentBranch,
    fetchAllRemotes,
    setFetchPrune,
    pushBranch,
    openPushDialog,
    closePushDialog,
    deleteRemoteBranch,
    openDeleteRemoteBranchDialog,
    closeDeleteRemoteBranchDialog,
  };
}
