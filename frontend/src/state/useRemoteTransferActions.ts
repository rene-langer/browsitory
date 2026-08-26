import { useCallback, useEffect, useRef } from "react";
import type { PullOutcome, RemoteAuthMode, RepoClient, TransferProgress } from "../ipc/RepoClient";
import { credentialFailureMessage } from "./useMutationRunner";
import type { RunMutation, RunMutationWithMessage, RunMutationWithOutcome } from "./useMutationRunner";
import type { AppState } from "./useAppState";

function transferFailureMessage(progress: TransferProgress): string {
  if (progress.errorKind === "MissingCredential") return credentialFailureMessage("missing credential");
  if (progress.errorKind === "CredentialStoreFailure") return credentialFailureMessage("credential keychain failure");
  if (progress.errorKind === "SshAgentFailure") return credentialFailureMessage("SSH agent failure");
  const isPush = progress.operation === "PushBranch" || progress.operation === "PushTags";
  if (isPush && progress.errorKind === "NonFastForward") {
    return "Push was rejected because the remote has newer commits. Pull or reconcile history, then try again.";
  }
  if (isPush && progress.errorKind === "RejectedRemoteRef") {
    return "The remote rejected the pushed reference.";
  }
  if (isPush) return "Push failed";
  if (progress.operation === "Pull") return "Pull failed";
  if (progress.operation === "Fetch") return "Fetch failed";
  return "Transfer failed";
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
  pushCurrentBranch(remoteName: string): Promise<void>;
  pushTags(remoteName: string, names: string[]): Promise<void>;
  pullCurrentUpstream(): Promise<void>;
  clearPendingPull(): void;
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
  runMutationWithMessage: RunMutationWithMessage,
  runMutationWithOutcome: RunMutationWithOutcome,
  setState: (updater: (prev: AppState) => AppState) => void,
): RemoteTransferActions {
  const activeTransferId = useRef<string | null>(null);
  const transferRequestPending = useRef(false);

  useEffect(() => {
    return client.subscribeTransferProgress((progress) => {
      if (progress.phase === "Starting") {
        if (!transferRequestPending.current || activeTransferId.current !== null) return;
        activeTransferId.current = progress.operationId;
        setState((prev) => ({ ...prev, transfer: progress }));
        return;
      }

      if (progress.operationId !== activeTransferId.current) return;

      if (progress.phase === "Completed") {
        transferRequestPending.current = false;
        void refresh().finally(() => {
          if (activeTransferId.current !== progress.operationId) return;
          activeTransferId.current = null;
          setState((prev) => ({ ...prev, transfer: null, pending: false }));
        });
        return;
      }

      if (progress.phase === "Failed") {
        transferRequestPending.current = false;
        activeTransferId.current = null;
        setState((prev) => ({
          ...prev,
          transfer: null,
          error: transferFailureMessage(progress),
          pending: false,
        }));
        return;
      }

      setState((prev) => ({ ...prev, transfer: progress }));
    });
  }, [client, refresh, repoPath, setState]);

  const addRemote = useCallback(
    (name: string, fetchUrl: string, pushUrl: string | null) =>
      runMutationWithMessage(() => client.addRemote(repoPath, name, fetchUrl, pushUrl)),
    [client, runMutationWithMessage, repoPath],
  );
  const renameRemote = useCallback(
    async (oldName: string, newName: string) => {
      let renamed = false;
      await runMutation(async () => {
        await client.renameRemote(repoPath, oldName, newName);
        renamed = true;
      });
      return renamed;
    },
    [client, runMutation, repoPath],
  );
  const updateRemoteUrls = useCallback(
    (name: string, fetchUrl: string, pushUrl: string | null) => runMutation(() => client.updateRemoteUrls(repoPath, name, fetchUrl, pushUrl)),
    [client, runMutation, repoPath],
  );
  const removeRemote = useCallback(
    (name: string, clearUpstreams: boolean) => runMutation(() => client.removeRemote(repoPath, name, clearUpstreams)),
    [client, runMutation, repoPath],
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
      runMutation(async () => {
        await client.setCurrentUpstream(repoPath, remoteName, remoteBranch);
        setState((prev) => ({ ...prev, pullOutcome: null }));
      }),
    [client, runMutation, repoPath, setState],
  );
  const clearCurrentUpstream = useCallback(
    () =>
      runMutation(async () => {
        await client.clearCurrentUpstream(repoPath);
        setState((prev) => ({ ...prev, pullOutcome: null }));
      }),
    [client, runMutation, repoPath, setState],
  );
  const startTransfer = useCallback(
    async (operation: TransferProgress["operation"], start: () => Promise<string>) => {
      try {
        transferRequestPending.current = true;
        activeTransferId.current = null;
        setState((prev) => ({ ...prev, pending: true, error: null }));
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
    (remoteName: string) => startTransfer("Fetch", () => client.fetchRemote(repoPath, remoteName)),
    [client, startTransfer, repoPath],
  );
  const listRemoteBranches = useCallback(
    (remoteName: string) => client.listRemoteBranches(repoPath, remoteName),
    [client, repoPath],
  );

  const createTag = useCallback(
    (name: string, message: string | null) => runMutationWithMessage(() => client.createTag(repoPath, name, message)),
    [client, runMutationWithMessage, repoPath],
  );
  const deleteTag = useCallback(
    (name: string) => runMutation(() => client.deleteTag(repoPath, name)),
    [client, runMutation, repoPath],
  );
  const pushCurrentBranch = useCallback(
    (remoteName: string) =>
      startTransfer("PushBranch", () => client.pushCurrentBranch(repoPath, remoteName)),
    [client, startTransfer, repoPath],
  );
  const pushTags = useCallback(
    (remoteName: string, names: string[]) =>
      startTransfer("PushTags", () => client.pushTags(repoPath, remoteName, names)),
    [client, startTransfer, repoPath],
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
  };
}
