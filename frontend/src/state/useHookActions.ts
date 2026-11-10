import { useCallback, useEffect, useRef } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";

export interface HookActions {
  setSkipHooks(on: boolean): void;
  // Cancels the hook running now (`state.hookRun`) through `cancelTransfer`; no-op when none is.
  cancelHookRun(): Promise<void>;
  openHookFailure(): void;
  closeHookFailure(): void;
  clearCommitMessageHandled(): void;
}

// Owns the hook-progress subscription (commit/amend/Continue, checkout, merge and pick hooks;
// pushes and pulls report through transfer progress) and the small pieces of hook UI state that
// are not tied to one action.
export function useHookActions(
  client: RepoClient,
  repoPath: string,
  hookRun: AppState["hookRun"],
  setState: (updater: (prev: AppState) => AppState) => void,
): HookActions {
  // One slot is enough: a repository has one worker thread (`WorkerRegistry`), which runs its
  // commands and their hooks one at a time, so a hook's Finished always comes before the next
  // Started.
  const hookRunRef = useRef(hookRun);
  useEffect(() => {
    hookRunRef.current = hookRun;
  }, [hookRun]);

  useEffect(
    () =>
      client.subscribeHookProgress((event) => {
        if (event.repoPath !== repoPath) return;
        if (event.phase === "Started") {
          const run = { operationId: event.operationId, hook: event.hook, source: event.source };
          // Set now, not only by the effect above: Cancel is on screen once React commits this
          // render, and a click before its effects run would otherwise cancel nothing (#182).
          hookRunRef.current = run;
          setState((prev) => ({ ...prev, hookRun: run }));
        } else {
          // Clear it here too: Started and Finished in one React batch leave `hookRun` null
          // before and after, so the effect above wouldn't run and Cancel would target the
          // ended run (#182).
          if (hookRunRef.current?.operationId === event.operationId) hookRunRef.current = null;
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
