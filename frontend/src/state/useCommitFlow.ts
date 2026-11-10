import { useCallback, useEffect, useRef } from "react";
import type { Trailer } from "../ipc/RepoClient";
import type { UseAppStateResult } from "./useAppState";
import type { ToastAction } from "./useToasts";

type CommitWith = (
  message: string,
  noVerify: boolean,
  trailers: Trailer[],
  onCommitted?: (commitId: string) => void,
) => Promise<boolean>;

/**
 * Where a commit lands, "Commit and push", and the toast after a commit (#651): "Committed
 * 4c5beaf to main" with Push, or with Undo when there is nowhere to push. `commitWith` is
 * `useCommitDraftActions`' `commitAndClearDraft`; `onUndo` is the workspace's Undo.
 *
 * The toast stack is app-level and outlives this workspace (a tab switch unmounts it), so a toast
 * action checks at click time that the workspace is still mounted and idle, and Undo also that
 * HEAD is still the commit it names: Undo undoes whatever came last.
 */
export function useCommitFlow(
  appState: UseAppStateResult,
  commitWith: CommitWith,
  pushToast: (message: string, action?: ToastAction) => void,
  onUndo: () => void,
) {
  const { branches, headInfo, upstream, remotes, skipHooks, pending, transfer } = appState.state;
  const branch = headInfo.detachedAt !== null ? null : (branches.find((b) => b.isCurrent)?.name ?? null);
  const commitAndPushReason =
    branch === null
      ? "Commit and push needs a branch: HEAD is detached."
      : remotes.length === 0
        ? "No remote configured."
        : null;

  const live = useRef({ mounted: true, busy: false, headId: headInfo.headId });
  useEffect(() => {
    const state = live.current;
    state.mounted = true;
    return () => {
      state.mounted = false;
    };
  }, []);
  useEffect(() => {
    live.current.busy = pending || transfer !== null;
    live.current.headId = headInfo.headId;
  }, [pending, transfer, headInfo.headId]);
  const idle = () => live.current.mounted && !live.current.busy;

  const { pushCurrentBranch, openDialog } = appState;
  // Like the sync bar's Push: to the upstream, or the Push dialog to publish the branch.
  const push = useCallback(() => {
    if (branch === null) return;
    if (upstream !== null) void pushCurrentBranch(upstream.remoteName);
    else openDialog({ kind: "push", localBranch: branch });
  }, [branch, upstream, pushCurrentBranch, openDialog]);

  const commit = useCallback(
    async (message: string, trailers: Trailer[], thenPush: boolean) => {
      // Set by the callback: a `let` the compiler can't see change, hence the cast.
      let committed = null as string | null;
      const ok = await commitWith(message, skipHooks, trailers, (id) => (committed = id));
      if (!ok || committed === null) return ok;
      const id = committed;
      const text = `Committed ${id.slice(0, 7)}${branch === null ? "" : ` to ${branch}`}`;
      if (thenPush) {
        // The push reports itself; undoing under it would be a surprise.
        pushToast(text);
        push();
      } else if (commitAndPushReason === null) {
        pushToast(text, {
          label: "Push",
          run: () => {
            if (idle()) push();
          },
        });
      } else {
        pushToast(text, {
          label: "Undo",
          run: () => {
            if (idle() && live.current.headId === id) onUndo();
          },
        });
      }
      return ok;
    },
    [commitWith, skipHooks, branch, pushToast, push, commitAndPushReason, onUndo],
  );

  const onCommit = useCallback((message: string, trailers: Trailer[]) => commit(message, trailers, false), [commit]);
  const onCommitAndPush = useCallback(
    (message: string, trailers: Trailer[]) => commit(message, trailers, true),
    [commit],
  );
  return { onCommit, commitFlow: { branch, onCommitAndPush, commitAndPushReason } };
}
