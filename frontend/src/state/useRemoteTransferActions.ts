import { useCallback, useEffect, useRef } from "react";
import { errorKind, type PullOutcome, type RemoteAuthMode, type RepoClient, type TransferProgress } from "../ipc/RepoClient";
import { recordFetch } from "../lib/autoFetch";
import { writeFetchPrune } from "../lib/fetchPrune";
import { upstreamPushBranch } from "../lib/pushTarget";
import { failureError, type MutationRunner } from "./useMutationRunner";
import type { RemoteInfo, TagInfo, UpstreamInfo } from "../ipc/RepoClient";
import { refNameClash, takenNameMessage } from "../lib/takenNames";
import type { AppState } from "./useAppState";
import { withoutDialog } from "./workspaceDialog";
import {
  IDLE_TRANSFER,
  finishCompletedTransfer,
  onTransferProgress,
  onTransferRejected,
  onTransferStarted,
  requestTransfer,
  transferRequest,
  type TransferRequest,
  type TransferSession,
} from "../lib/transferSession";
import type { TransferSubject } from "../lib/transferCopy";

/** What the actions read from the repository state when they run, not when they were made. */
export interface RemoteTransferSnapshot {
  upstream: UpstreamInfo | null;
  fetchPrune: boolean | null;
  currentBranch: string | null;
  remotes: RemoteInfo[];
  tags: TagInfo[];
}

export interface RemoteTransferContext {
  client: RepoClient;
  repoPath: string;
  refresh: () => Promise<void>;
  runners: Pick<
    MutationRunner,
    | "runMutation"
    | "runMutationWithOutcome"
    | "runOptimisticMutation"
    | "runOptimisticMutationWithMessage"
    | "runOptimisticMutationWithOutcome"
  >;
  setState: (updater: (prev: AppState) => AppState) => void;
  snapshot: RemoteTransferSnapshot;
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
  // `Promise<string | null>` for the same reason as `addRemote` above. `target` is the commit to
  // tag (a full id), `null` for HEAD's commit.
  createTag(name: string, message: string | null, target: string | null): Promise<string | null>;
  deleteTag(name: string): Promise<void>;
  // `noVerify` skips the `pre-push` hook (the hook-failure dialog's retry); default `false`.
  pushCurrentBranch(remoteName: string, noVerify?: boolean): Promise<void>;
  pushTags(remoteName: string, names: string[], noVerify?: boolean): Promise<void>;
  pullCurrentUpstream(): Promise<void>;
  clearPendingPull(): void;
  openAddRemoteDraft(): void;
  closeAddRemoteDraft(): void;
  // Closes the force-push confirmation (`dialog` kind "forcePush") and drops the offer.
  cancelForcePush(): void;
  forcePushCurrentBranch(remoteName: string, noVerify?: boolean): Promise<void>;
  fetchAllRemotes(): Promise<void>;
  // Persists the Prune choice for this repo; every later fetch passes it explicitly.
  setFetchPrune(prune: boolean): void;
  pushBranch(remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean, noVerify?: boolean): Promise<void>;
  deleteRemoteBranch(remoteName: string, branchName: string, noVerify?: boolean): Promise<void>;
  // Deletes `tagName` on `remoteName`. With `deleteLocal`, the local tag is deleted (an undoable
  // step of its own) once the remote delete has landed; a failed remote delete keeps it.
  deleteRemoteTag(remoteName: string, tagName: string, deleteLocal: boolean, noVerify?: boolean): Promise<void>;
}

/**
 * A transfer still running when its tab's workspace unmounted (only the active tab is mounted,
 * PERF-001): the tab picks it up again when it comes back (#659). Module-level, like the last
 * fetch times, so it outlives the workspace.
 */
const detachedTransfers = new Map<string, { session: TransferSession; transfer: TransferProgress | null }>();

/** Forgets every detached transfer: for tests. */
export function resetDetachedTransfers() {
  detachedTransfers.clear();
}

// Owns everything transfer-progress-shaped (fetch/push/pull), plus the rest of remote/upstream/tag
// management that shares its `startTransfer`/credential-failure plumbing — the "remotes/transfer"
// domain named in MAINT-002. Also owns the `subscribeTransferProgress` effect. The transfer's
// lifecycle (which events belong to the transfer this tab started, and what its end shows) is
// `lib/transferSession.ts`; this hook only feeds it and applies what it returns.
export function useRemoteTransferActions(context: RemoteTransferContext): RemoteTransferActions {
  const { client, repoPath, refresh, setState, snapshot } = context;
  const {
    runMutation,
    runMutationWithOutcome,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
    runOptimisticMutationWithOutcome,
  } = context.runners;
  const session = useRef<TransferSession>(IDLE_TRANSFER);
  // The progress last shown, kept for a tab that unmounts mid-transfer.
  const shownProgress = useRef<TransferProgress | null>(null);
  // The latest snapshot, for the actions and the progress listener, which outlive a render.
  const latest = useRef<RemoteTransferSnapshot>(snapshot);
  useEffect(() => {
    latest.current = snapshot;
  });

  const deleteTag = useCallback(
    (name: string) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, tags: prev.tags.filter((tag) => tag.name !== name) }),
        () => client.deleteTag(repoPath, name),
      ),
    [client, runOptimisticMutation, repoPath],
  );

  useEffect(() => {
    return client.subscribeTransferProgress((progress) => {
      const step = onTransferProgress(session.current, progress, latest.current.upstream);
      session.current = step.session;
      const effect = step.effect;
      switch (effect.kind) {
        case "none":
          return;
        case "show":
          shownProgress.current = effect.transfer;
          setState((prev) => ({ ...prev, transfer: effect.transfer }));
          return;
        case "failed":
          setState((prev) => ({ ...prev, ...effect.patch }));
          return;
        case "completed":
          if (effect.recordFetch) recordFetch(repoPath);
          setState((prev) => ({ ...prev, forcePushOffer: null }));
          void refresh().finally(() => {
            const finish = finishCompletedTransfer(session.current, effect.operationId);
            session.current = finish.session;
            if (!finish.finished) return;
            // Set after `refresh()` (the transfer cleared `error` when it started; a refresh only
            // clears an error a failed refresh set) and in the same update that clears `transfer`,
            // so a partial Fetch all both reports the dead remotes and suppresses App's "Fetch
            // complete" toast.
            // A completed pull reports its outcome itself (`useOutcomeToasts`); a partial Fetch all
            // reports its error instead of a toast.
            const subject = finish.subject;
            const toast = subject !== null && subject.kind !== "pull" && effect.error === null;
            setState((prev) => ({
              ...prev,
              transfer: null,
              transferSubject: null,
              transferOutcome: toast ? { subject } : prev.transferOutcome,
              pending: false,
              error: effect.error ?? prev.error,
            }));
            if (finish.deleteLocalTag !== null) void deleteTag(finish.deleteLocalTag);
          });
          return;
      }
    });
  }, [client, deleteTag, refresh, repoPath, setState]);

  // A tab switched away from mid-transfer, and back (#659): the transfer is still running, or has
  // ended while nobody listened. Show it again and keep following its events; the worker runs one
  // command at a time, so once a read sent now answers, the transfer is over, and a terminal event
  // missed meanwhile can't leave the tab busy for good. Saved again when the tab goes away.
  // Runs on mount, unmount and a repository change only: a new `client` object (a caller that
  // builds one per render) must not save and restore the running transfer in a loop.
  const latestClient = useRef(client);
  useEffect(() => {
    latestClient.current = client;
  });
  useEffect(() => {
    const detached = detachedTransfers.get(repoPath);
    detachedTransfers.delete(repoPath);
    if (detached !== undefined && detached.session.status !== "idle" && detached.session.status !== "completing") {
      const restored = detached.session;
      session.current = restored;
      shownProgress.current = detached.transfer;
      setState((prev) => ({ ...prev, pending: true, transfer: detached.transfer, transferSubject: restored.request.subject }));
      void latestClient.current
        .getHeadInfo(repoPath)
        .catch(() => {})
        .then(() => {
          if (session.current !== restored) return;
          session.current = IDLE_TRANSFER;
          setState((prev) => ({ ...prev, pending: false, transfer: null, transferSubject: null }));
        });
    }
    return () => {
      const running = session.current;
      if (running.status === "requested" || running.status === "active") {
        detachedTransfers.set(repoPath, { session: running, transfer: shownProgress.current });
      }
    };
  }, [repoPath, setState]);

  // Refuses `name` when `existing` already holds it, reporting it like any failed mutation (also
  // in `state.error`); returns the message, or `null` when the name is free. `folders` also
  // refuses a name that is a folder of a listed one or the other way round (`v1` and `v1/rc`),
  // which git refuses for refs; remote names aren't refs, so remotes compare exactly.
  const refuseTakenName = useCallback(
    (kind: string, existing: string[], name: string, folders: boolean): string | null => {
      const clash = folders ? refNameClash(existing, name) : existing.includes(name) ? name : null;
      if (clash === null) return null;
      const message = takenNameMessage(kind, name, clash);
      setState((prev) => ({ ...prev, error: message }));
      return message;
    },
    [setState],
  );
  const addRemote = useCallback(
    async (name: string, fetchUrl: string, pushUrl: string | null) => {
      // A taken name is refused here, before the optimistic append: appending it would render the
      // remote twice (with a duplicate React key in `BranchTree`) until the backend refused it, with
      // libgit2's raw "already exists" error (#408).
      const refused = refuseTakenName("remote", latest.current.remotes.map((remote) => remote.name), name, false);
      if (refused !== null) return refused;
      return runOptimisticMutationWithMessage(
        (prev) =>
          // Belt and braces for a name that arrived since the check above: never list it twice.
          prev.remotes.some((remote) => remote.name === name)
            ? prev
            : {
                ...prev,
                remotes: [...prev.remotes, { name, fetchUrl, pushUrl, authMode: null, authUsername: null }],
              },
        () => client.addRemote(repoPath, name, fetchUrl, pushUrl),
      );
    },
    [client, refuseTakenName, runOptimisticMutationWithMessage, repoPath],
  );
  const renameRemote = useCallback(
    async (oldName: string, newName: string) => {
      // As `addRemote`: renaming onto another remote's name would list that name twice (#413).
      const others = latest.current.remotes.map((remote) => remote.name).filter((name) => name !== oldName);
      if (refuseTakenName("remote", others, newName, false) !== null) return false;
      return runOptimisticMutationWithOutcome(
        (prev) =>
          prev.remotes.some((remote) => remote.name === newName && remote.name !== oldName)
            ? prev
            : {
                ...prev,
                remotes: prev.remotes.map((remote) => (remote.name === oldName ? { ...remote, name: newName } : remote)),
              },
        () => client.renameRemote(repoPath, oldName, newName),
      );
    },
    [client, refuseTakenName, runOptimisticMutationWithOutcome, repoPath],
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
    async (request: TransferRequest, start: () => Promise<string>) => {
      try {
        session.current = requestTransfer(request);
        setState((prev) => ({
          ...prev,
          pending: true,
          transferSubject: request.subject,
          error: null,
          forcePushOffer: null,
          hookFailure: null,
        }));
        const operationId = await start();
        const step = onTransferStarted(session.current, operationId);
        session.current = step.session;
        const transfer = step.transfer;
        if (transfer !== null) {
          shownProgress.current = transfer;
          setState((prev) => ({ ...prev, transfer }));
        }
      } catch (err) {
        session.current = onTransferRejected();
        setState((prev) => ({ ...prev, ...failureError(err), pending: false, transferSubject: null }));
      }
    },
    [setState],
  );

  const fetchRemote = useCallback(
    (remoteName: string) => {
      return startTransfer(transferRequest("Fetch", { subject: { kind: "fetch", remoteName } }), () =>
        client.fetchRemote(repoPath, remoteName, latest.current.fetchPrune),
      );
    },
    [client, startTransfer, repoPath],
  );
  const fetchAllRemotes = useCallback(() => {
    const request = transferRequest("FetchAll", {
      subject: { kind: "fetchAll", remoteCount: latest.current.remotes.length },
    });
    return startTransfer(request, () => client.fetchAllRemotes(repoPath, latest.current.fetchPrune));
  }, [client, startTransfer, repoPath]);
  const setFetchPrune = useCallback(
    (prune: boolean) => {
      writeFetchPrune(repoPath, prune);
      latest.current = { ...latest.current, fetchPrune: prune };
      setState((prev) => ({ ...prev, fetchPrune: prune }));
    },
    [repoPath, setState],
  );
  const listRemoteBranches = useCallback(
    (remoteName: string) => client.listRemoteBranches(repoPath, remoteName),
    [client, repoPath],
  );

  const createTag = useCallback(
    async (name: string, message: string | null, target: string | null) => {
      // A taken tag name would show twice in `TagPanel` (keyed by name) until refused (#413).
      const refused = refuseTakenName("tag", latest.current.tags.map((tag) => tag.name), name, true);
      if (refused !== null) return refused;
      return runOptimisticMutationWithMessage(
        (prev) =>
          refNameClash(prev.tags.map((tag) => tag.name), name) !== null
            ? prev
            : {
                ...prev,
                tags: [
                  ...prev.tags,
                  { name, targetId: "", commitId: null, annotated: message !== null, message, taggerName: null, timestamp: null },
                ],
              },
        () => client.createTag(repoPath, name, message, target),
      );
    },
    [client, refuseTakenName, runOptimisticMutationWithMessage, repoPath],
  );
  const pushCurrentBranch = useCallback(
    async (remoteName: string, noVerify = false) => {
      // push.default does not choose where the current branch goes on its upstream's remote
      // (e.g. `simple` with a differently named upstream): ask with Push to… (prefilled with
      // that remote and the local name) instead of pushing. The backend refuses such a push too.
      const { upstream, currentBranch } = latest.current;
      if (upstream !== null && upstream.remoteName === remoteName && upstreamPushBranch(upstream) === null) {
        const localBranch = currentBranch ?? upstream.localBranch;
        setState((prev) => ({ ...prev, dialog: { kind: "push", localBranch } }));
        return;
      }
      const onUpstream = upstream !== null && upstream.remoteName === remoteName;
      const localBranch = currentBranch ?? upstream?.localBranch ?? "HEAD";
      const request = transferRequest("PushBranch", {
        subject: {
          kind: "push",
          localBranch,
          target: onUpstream ? `${remoteName}/${upstreamPushBranch(upstream) ?? localBranch}` : remoteName,
          // The upstream's ahead count is what this push sends only when it goes to the upstream.
          ahead: onUpstream && upstreamPushBranch(upstream) === upstream.remoteBranch ? (upstream.ahead ?? null) : null,
          setUpstream: false,
          force: false,
        },
        push: { operation: "PushBranch", remoteName },
        forcePushOfferRemote: remoteName,
      });
      return startTransfer(request, () => client.pushCurrentBranch(repoPath, remoteName, noVerify));
    },
    [client, startTransfer, repoPath, setState],
  );
  const cancelForcePush = useCallback(
    () => setState((prev) => ({ ...prev, forcePushOffer: null, dialog: withoutDialog(prev.dialog, "forcePush") })),
    [setState],
  );
  const forcePushCurrentBranch = useCallback(
    (remoteName: string, noVerify = false) => {
      // The lease targets what `force_push_current_branch` resolves under push.default (see
      // `lib/pushTarget.ts`), else the current branch's own name.
      // A stale-lease failure names the leased branch, else the current one.
      const { upstream, currentBranch } = latest.current;
      const leased = upstream !== null && upstream.remoteName === remoteName ? upstreamPushBranch(upstream) : null;
      setState((prev) => ({ ...prev, forcePushOffer: null, dialog: withoutDialog(prev.dialog, "forcePush") }));
      const forcePushTarget = { remoteName, branch: leased ?? currentBranch ?? upstream?.localBranch ?? null };
      const request = transferRequest("ForcePushBranch", {
        subject: {
          kind: "push",
          localBranch: currentBranch ?? upstream?.localBranch ?? "HEAD",
          target: forcePushTarget.branch === null ? remoteName : `${remoteName}/${forcePushTarget.branch}`,
          ahead: null,
          setUpstream: false,
          force: true,
        },
        push: { operation: "ForcePushBranch", remoteName },
        forcePushTarget,
      });
      return startTransfer(request, () => client.forcePushCurrentBranch(repoPath, remoteName, noVerify));
    },
    [client, startTransfer, repoPath, setState],
  );
  const pushTags = useCallback(
    (remoteName: string, names: string[], noVerify = false) => {
      const request = transferRequest("PushTags", {
        subject: { kind: "pushTags", remoteName, names },
        push: { operation: "PushTags", remoteName, names },
      });
      return startTransfer(request, () => client.pushTags(repoPath, remoteName, names, noVerify));
    },
    [client, startTransfer, repoPath],
  );
  const pushBranch = useCallback(
    (remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean, noVerify = false) => {
      // The force-push offer force-pushes the *current* branch to where force_push_current_branch
      // resolves it: only offer it when that is exactly what this push targeted.
      const { upstream, currentBranch } = latest.current;
      const forceTarget =
        upstream !== null && upstream.remoteName === remoteName ? upstreamPushBranch(upstream) : localBranch;
      const offersForcePush = forceTarget !== null && localBranch === currentBranch && remoteBranch === forceTarget;
      setState((prev) => ({ ...prev, dialog: withoutDialog(prev.dialog, "push") }));
      const toUpstream =
        upstream !== null &&
        localBranch === upstream.localBranch &&
        upstream.remoteName === remoteName &&
        upstream.remoteBranch === remoteBranch;
      const request = transferRequest("PushBranch", {
        subject: {
          kind: "push",
          localBranch,
          target: `${remoteName}/${remoteBranch}`,
          ahead: toUpstream ? (upstream.ahead ?? null) : null,
          setUpstream,
          force: false,
        },
        push: { operation: "PushBranchTo", remoteName, localBranch, remoteBranch, setUpstream },
        forcePushOfferRemote: offersForcePush ? remoteName : null,
      });
      return startTransfer(request, () =>
        client.pushBranch(repoPath, remoteName, localBranch, remoteBranch, setUpstream, noVerify),
      );
    },
    [client, startTransfer, repoPath, setState],
  );
  const deleteRemoteBranch = useCallback(
    (remoteName: string, branchName: string, noVerify = false) => {
      setState((prev) => ({ ...prev, dialog: withoutDialog(prev.dialog, "deleteRemoteBranch") }));
      const request = transferRequest("DeleteRemoteBranch", {
        subject: { kind: "deleteRemoteBranch", remoteName, branchName },
        push: { operation: "DeleteRemoteBranch", remoteName, branchName },
      });
      return startTransfer(request, () =>
        client.deleteRemoteBranch(repoPath, remoteName, branchName, noVerify),
      );
    },
    [client, startTransfer, repoPath, setState],
  );
  const deleteRemoteTag = useCallback(
    (remoteName: string, tagName: string, deleteLocal: boolean, noVerify = false) => {
      setState((prev) => ({ ...prev, dialog: withoutDialog(prev.dialog, "deleteRemoteTag") }));
      const request = transferRequest("DeleteRemoteTag", {
        subject: { kind: "deleteRemoteTag", remoteName, tagName, deleteLocal },
        push: { operation: "DeleteRemoteTag", remoteName, tagName, deleteLocal },
        deleteLocalTag: deleteLocal ? tagName : null,
      });
      return startTransfer(request, () => client.deleteRemoteTag(repoPath, remoteName, tagName, noVerify));
    },
    [client, startTransfer, repoPath, setState],
  );

  const pullCurrentUpstream = useCallback(async () => {
    try {
      // No operation id comes back: the pull's own Starting event is adopted.
      const { upstream } = latest.current;
      const subject: TransferSubject = {
        kind: "pull",
        target: upstream === null ? "the upstream" : `${upstream.remoteName}/${upstream.remoteBranch}`,
      };
      session.current = requestTransfer(transferRequest("Pull", { subject }));
      setState((prev) => ({
        ...prev,
        pending: true,
        transferSubject: subject,
        error: null,
        pendingPull: null,
        pullOutcome: null,
      }));
      const outcome: PullOutcome = await client.pullCurrentUpstream(repoPath);
      if (outcome.kind === "Diverged" || outcome.kind === "NoFastForward") {
        const pendingPull =
          outcome.kind === "Diverged"
            ? { upstreamRef: outcome.upstreamRef }
            : { upstreamRef: outcome.upstreamRef, noFastForward: true as const };
        setState((prev) => ({ ...prev, pending: false, transferSubject: null, pendingPull }));
        return;
      }
      // A pull fetches the upstream's remote first.
      recordFetch(repoPath);
      await refresh();
      setState((prev) => ({ ...prev, pending: false, transferSubject: null, pullOutcome: outcome }));
    } catch (err) {
      // The pull's terminal transfer event says why (`transferFailureMessage`), so it is shown
      // from there: a credential remediation, or "Pull cancelled.".
      const kind = errorKind(err);
      if (kind === "PullFailed" || kind === "Cancelled") return;
      session.current = onTransferRejected();
      setState((prev) => ({
        ...prev,
        transfer: null,
        transferSubject: null,
        pending: false,
        pendingPull: null,
        pullOutcome: null,
        ...(kind === "DirtyWorktree"
          ? { error: "Commit or stash your changes before pulling.", errorHint: null }
          : failureError(err)),
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
    cancelForcePush,
    forcePushCurrentBranch,
    fetchAllRemotes,
    setFetchPrune,
    pushBranch,
    deleteRemoteBranch,
    deleteRemoteTag,
  };
}
