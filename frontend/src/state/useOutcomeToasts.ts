import { useEffect } from "react";
import type { AppState } from "./useAppState";

/** Toasts for a finished pull, merge or stash apply/pop (`RepoWorkspace`). */
export function useOutcomeToasts(state: AppState, pushToast: (message: string) => void) {
  const pullOutcome = state.pullOutcome;
  useEffect(() => {
    if (pullOutcome === null) return;
    if (pullOutcome.kind === "UpToDate") pushToast("Already up to date");
    else if (pullOutcome.kind === "FastForwarded") pushToast(`Pulled ${pullOutcome.upstreamRef}`);
  }, [pullOutcome, pushToast]);
  const mergeOutcome = state.mergeOutcome;
  useEffect(() => {
    if (mergeOutcome === null) return;
    const { branchName, outcome } = mergeOutcome;
    if (outcome.kind === "UpToDate") pushToast("Already up to date");
    else if (outcome.kind === "FastForwarded") pushToast(`Fast-forwarded to ${branchName}`);
    else if (outcome.kind === "Squashed") pushToast(`Squashed ${branchName}. Review and commit.`);
  }, [mergeOutcome, pushToast]);
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
      );
  }, [stashOutcome, pushToast]);
}
