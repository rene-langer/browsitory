import { useCallback, useEffect, useRef } from "react";
import type { Trailer } from "../ipc/RepoClient";
import type { UseAppStateResult } from "./useAppState";
import type { PushToast, ToastAction } from "./useToasts";

type CommitWith = (
  message: string,
  noVerify: boolean,
  trailers: Trailer[],
  onCommitted?: (commitId: string) => void,
) => Promise<boolean>;

/**
 * Where a commit lands, "Commit and push", and the toast after a commit (#651): "Committed
 * 4c5beaf to main" with Push and Undo (#733), or Undo alone when there is nowhere to push. `commitWith` is
 * `useCommitDraftActions`' `commitAndClearDraft`; `undoToastAction` is `useUndoRedoActions`'
 * (#725), whose Undo runs only while the commit is still the newest undo step.
 *
 * The toast stack is app-level and outlives this workspace (a tab switch unmounts it), so Push
 * checks at click time that the workspace is still mounted and idle, and that the branch it
 * names is still the current one (#735): it never pushes a branch checked out since.
 */
export function useCommitFlow(
  appState: UseAppStateResult,
  commitWith: CommitWith,
  pushToast: PushToast,
  undoToastAction: () => ToastAction,
) {
  const { branches, headInfo, upstream, remotes, skipHooks, pending, transfer } = appState.state;
  const branch = headInfo.detachedAt !== null ? null : (branches.find((b) => b.isCurrent)?.name ?? null);
  const commitAndPushReason =
    branch === null
      ? "Commit and push needs a branch: HEAD is detached."
      : remotes.length === 0
        ? "No remote configured."
        : null;

  const live = useRef({ mounted: true, busy: false, branch: null as string | null });
  useEffect(() => {
    const state = live.current;
    state.mounted = true;
    return () => {
      state.mounted = false;
    };
  }, []);
  useEffect(() => {
    live.current.busy = pending || transfer !== null;
  }, [pending, transfer]);
  useEffect(() => {
    live.current.branch = branch;
  }, [branch]);
  const idle = () => live.current.mounted && !live.current.busy;

  const { pushCurrentBranch, openDialog } = appState;
  // Like the sync bar's Push: to the upstream, or the Push dialog to publish the branch.
  const push = useCallback(() => {
    if (branch === null) return;
    if (upstream !== null) void pushCurrentBranch(upstream.remoteName);
    else openDialog({ kind: "push", localBranch: branch });
  }, [branch, upstream, pushCurrentBranch, openDialog]);

  // The last commit that didn't land, for "Save and commit" in the identity dialog (#689).
  const lastAttempt = useRef<{ message: string; trailers: Trailer[]; thenPush: boolean } | null>(null);
  const commit = useCallback(
    async (message: string, trailers: Trailer[], thenPush: boolean) => {
      // Set by the callback: a `let` the compiler can't see change, hence the cast.
      let committed = null as string | null;
      lastAttempt.current = { message, trailers, thenPush };
      const ok = await commitWith(message, skipHooks, trailers, (id) => (committed = id));
      if (ok) lastAttempt.current = null;
      if (!ok || committed === null) return ok;
      const id = committed;
      const text = `Committed ${id.slice(0, 7)}${branch === null ? "" : ` to ${branch}`}`;
      if (thenPush) {
        // The push reports itself; undoing under it would be a surprise.
        pushToast(text, { tone: "success" });
        push();
      } else if (commitAndPushReason === null) {
        const committedTo = branch;
        pushToast(text, {
          tone: "success",
          actions: [
            {
              label: "Push",
              run: () => {
                if (!idle()) return;
                if (live.current.branch !== committedTo) {
                  pushToast(`Not pushed: ${committedTo} is no longer the current branch.`, { tone: "info" });
                  return;
                }
                push();
              },
            },
            undoToastAction(),
          ],
        });
      } else {
        pushToast(text, { tone: "success", action: undoToastAction() });
      }
      return ok;
    },
    [commitWith, skipHooks, branch, pushToast, push, commitAndPushReason, undoToastAction],
  );

  const onCommit = useCallback((message: string, trailers: Trailer[]) => commit(message, trailers, false), [commit]);
  const onCommitAndPush = useCallback(
    (message: string, trailers: Trailer[]) => commit(message, trailers, true),
    [commit],
  );
  const { identityRequired } = appState.state;
  const { setIdentity, dismissIdentity } = appState;
  const retrying = identityRequired === "commit";
  const onSaveIdentity = useCallback(
    async (name: string, email: string, global: boolean) => {
      const failure = await setIdentity(name, email, global);
      if (failure !== null) return failure;
      const attempt = identityRequired === "commit" ? lastAttempt.current : null;
      if (attempt === null) pushToast("Git identity saved. Commit again.", { tone: "success" });
      else void commit(attempt.message, attempt.trailers, attempt.thenPush);
      return null;
    },
    [setIdentity, identityRequired, commit, pushToast],
  );
  const identity = {
    required: identityRequired !== null,
    retrying,
    onSave: onSaveIdentity,
    onCancel: dismissIdentity,
  };
  return { onCommit, commitFlow: { branch, onCommitAndPush, commitAndPushReason, identity } };
}
