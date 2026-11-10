import { useEffect } from "react";
import type { AppState } from "./useAppState";
import type { PushToast, ToastAction } from "./useToasts";

/** `refs/remotes/origin/main` → `origin/main`, as the branch list shows it. */
function shortRef(ref: string): string {
  return ref.replace(/^refs\/(remotes|heads)\//, "");
}

/**
 * Toasts for a finished pull, merge, stash apply/pop, one-click history edit or patch action
 * (`RepoWorkspace`). Those that recorded an undo step offer Undo (#662): a fast-forward, a
 * squash merge, a history edit and a change applied to the working tree. Stash apply and pop
 * record none.
 */
export function useOutcomeToasts(state: AppState, pushToast: PushToast, undoToastAction: () => ToastAction) {
  const pullOutcome = state.pullOutcome;
  useEffect(() => {
    if (pullOutcome === null) return;
    if (pullOutcome.kind === "UpToDate") pushToast("Already up to date", { tone: "info" });
    else if (pullOutcome.kind === "FastForwarded")
      pushToast(`Pulled ${shortRef(pullOutcome.upstreamRef)}`, { action: undoToastAction() });
  }, [pullOutcome, pushToast, undoToastAction]);
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
