import { useCallback, useEffect, useRef } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";

export interface HookActions {
  setSkipHooks(on: boolean): void;
  cancelHookRun(): Promise<void>;
  openHookFailure(): void;
  closeHookFailure(): void;
  clearCommitMessageHandled(): void;
}

// Owns the hook-progress subscription (commit/amend; pushes report through transfer progress)
// and the small pieces of hook UI state that are not tied to one action.
export function useHookActions(
  client: RepoClient,
  repoPath: string,
  hookRun: AppState["hookRun"],
  setState: (updater: (prev: AppState) => AppState) => void,
): HookActions {
  const hookRunRef = useRef(hookRun);
  useEffect(() => {
    hookRunRef.current = hookRun;
  }, [hookRun]);

  useEffect(
    () =>
      client.subscribeHookProgress((event) => {
        if (event.repoPath !== repoPath) return;
        if (event.phase === "Started") {
          setState((prev) => ({ ...prev, hookRun: { operationId: event.operationId, hook: event.hook } }));
        } else {
          setState((prev) =>
            prev.hookRun?.operationId === event.operationId ? { ...prev, hookRun: null } : prev,
          );
        }
      }),
    [client, repoPath, setState],
  );

  const setSkipHooks = useCallback((on: boolean) => setState((prev) => ({ ...prev, skipHooks: on })), [setState]);
  const cancelHookRun = useCallback(async () => {
    const run = hookRunRef.current;
    if (run !== null) await client.cancelTransfer(repoPath, run.operationId);
  }, [client, repoPath]);
  const setDialogOpen = useCallback(
    (open: boolean) =>
      setState((prev) =>
        prev.hookFailure === null ? prev : { ...prev, hookFailure: { ...prev.hookFailure, dialogOpen: open } },
      ),
    [setState],
  );
  const openHookFailure = useCallback(() => setDialogOpen(true), [setDialogOpen]);
  const closeHookFailure = useCallback(() => setDialogOpen(false), [setDialogOpen]);
  const clearCommitMessageHandled = useCallback(
    () => setState((prev) => ({ ...prev, clearCommitMessage: null })),
    [setState],
  );
  return { setSkipHooks, cancelHookRun, openHookFailure, closeHookFailure, clearCommitMessageHandled };
}
