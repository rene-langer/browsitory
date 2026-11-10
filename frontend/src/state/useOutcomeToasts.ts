import { useEffect, useRef } from "react";
import { pullToast, transferToast } from "../lib/transferCopy";
import type { AppState } from "./useAppState";
import type { PushToast, ToastAction } from "./useToasts";

/** `refs/remotes/origin/main` → `origin/main`, as the branch list shows it. */
function shortRef(ref: string): string {
  return ref.replace(/^refs\/(remotes|heads)\//, "");
}

/**
 * Toasts for a finished fetch, push, pull, merge, stash apply/pop, one-click history edit or patch action
 * (`RepoWorkspace`). Those that recorded an undo step offer Undo (#662): a fast-forward, a
 * squash merge, a history edit and a change applied to the working tree. Stash apply and pop
 * record none.
 */
export function useOutcomeToasts(
  state: AppState,
  pushToast: PushToast,
  undoToastAction: () => ToastAction,
  /** Pulls the current branch: offered by a fetch that left it behind its upstream. */
  onPull?: () => void,
) {
  const { pullOutcome, transferOutcome, upstream } = state;
  // The upstream as the outcome's refresh left it, read when the outcome arrives (not a trigger).
  const latestUpstream = useRef(upstream);
  useEffect(() => {
    latestUpstream.current = upstream;
  });
  useEffect(() => {
    if (pullOutcome === null || pullOutcome.kind === "Diverged") return;
    const toast = pullToast(pullOutcome, latestUpstream.current);
    // A fast-forward is an undo step (#662); "up to date" changed nothing.
    pushToast(toast.message, {
      tone: toast.tone,
      action: pullOutcome.kind === "FastForwarded" ? undoToastAction() : undefined,
    });
  }, [pullOutcome, pushToast, undoToastAction]);
  // A fetch or push that completed (#661). Neither is an undo step, so neither offers Undo; a
  // fetch that left the branch behind offers Pull instead.
  const latestPull = useRef(onPull);
  useEffect(() => {
    latestPull.current = onPull;
  });
  useEffect(() => {
    if (transferOutcome === null) return;
    const toast = transferToast(transferOutcome.subject, latestUpstream.current);
    if (toast === null) return;
    const pull = latestPull.current;
    pushToast(toast.message, {
      tone: toast.tone,
      action: toast.offerPull && pull !== undefined ? { label: "Pull", run: pull } : undefined,
    });
  }, [transferOutcome, pushToast]);
  const mergeOutcome = state.mergeOutcome;
  useEffect(() => {
    if (mergeOutcome === null) return;
    const { outcome } = mergeOutcome;
    const branchName = shortRef(mergeOutcome.branchName);
    if (outcome.kind === "UpToDate") pushToast("Already up to date", { tone: "info" });
    else if (outcome.kind === "FastForwarded")
      pushToast(`Fast-forwarded to ${branchName}`, { action: undoToastAction() });
    else if (outcome.kind === "Squashed")
      pushToast(`Squashed ${branchName}. Review and commit.`, { action: undoToastAction() });
  }, [mergeOutcome, pushToast, undoToastAction]);
  const stashOutcome = state.stashOutcome;
  useEffect(() => {
    if (stashOutcome === null) return;
    const { action, outcome } = stashOutcome;
    if (outcome.kind === "Applied") pushToast(action === "pop" ? "Stash popped" : "Stash applied");
    else
      pushToast(
        action === "pop"
          ? "Stash applied with conflicts; the stash was kept. Resolve the files, then drop it."
          : "Stash applied with conflicts. Resolve the files.",
        { tone: "warning" },
      );
  }, [stashOutcome, pushToast]);
  const historyEditOutcome = state.historyEditOutcome;
  useEffect(() => {
    if (historyEditOutcome !== null) pushToast(historyEditOutcome.message, { action: undoToastAction() });
  }, [historyEditOutcome, pushToast, undoToastAction]);
  const patchOutcome = state.patchOutcome;
  useEffect(() => {
    if (patchOutcome === null) return;
    pushToast(patchOutcome.message, patchOutcome.undoable ? { action: undoToastAction() } : undefined);
  }, [patchOutcome, pushToast, undoToastAction]);
}
