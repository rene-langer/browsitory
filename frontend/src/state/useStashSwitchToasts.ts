import { useEffect } from "react";
import type { AppState } from "./useAppState";
import type { PushToast, ToastAction } from "./useToasts";

/**
 * The toast after "Stash and switch" (#653). It is one undo step unless re-applying left
 * conflicts (the index then has conflicts, so nothing was recorded), so the others offer Undo.
 */
export function useStashSwitchToasts(state: AppState, pushToast: PushToast, undoToastAction: () => ToastAction) {
  const done = state.stashSwitchOutcome;
  useEffect(() => {
    if (done === null) return;
    const { branchName, outcome } = done;
    switch (outcome.kind) {
      case "Reapplied":
        pushToast(`Switched to ${branchName} with your changes`, { action: undoToastAction() });
        break;
      case "Stashed":
        pushToast(`Switched to ${branchName}. Your changes are in the stash list.`, { action: undoToastAction() });
        break;
      case "Conflicted": {
        const count = outcome.files.length;
        pushToast(
          `Switched to ${branchName}. Your changes conflict with ${count === 1 ? "1 file" : `${count} files`} there: resolve them. They stay in the stash list too.`,
          { tone: "warning" },
        );
        break;
      }
      case "NotReapplied":
        pushToast(`Switched to ${branchName}. Your changes stay in the stash list: ${outcome.reason}`, {
          tone: "warning",
          action: undoToastAction(),
        });
        break;
    }
  }, [done, pushToast, undoToastAction]);
}
