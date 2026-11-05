import { useCallback, useEffect, useRef } from "react";
import type { UseAppStateResult } from "./useAppState";

/**
 * Wraps a mutation so its success shows `label` as a toast (`RepoWorkspace`'s `announce`).
 */
export function useAnnounce(appState: UseAppStateResult, pushToast: (message: string) => void) {
  // Positive feedback (AUD-2026-09-20-FB-003). The mutation runner swallows failures into
  // `state.error`, so success is detected as "pending fell back to false with no error" after a
  // labelled action; `announce` clears any stale error first so that check is unambiguous.
  const pendingLabel = useRef<string | null>(null);
  const { pending: statePending, error: stateError } = appState.state;
  useEffect(() => {
    if (statePending || pendingLabel.current === null) return;
    const label = pendingLabel.current;
    pendingLabel.current = null;
    if (stateError === null) pushToast(label);
  }, [statePending, stateError, pushToast]);
  const { dismissError } = appState;
  const announce = useCallback(
    <A extends unknown[], R,>(label: string, action: (...args: A) => R) =>
      (...args: A): R => {
        dismissError();
        pendingLabel.current = label;
        return action(...args);
      },
    [dismissError],
  );
  return announce;
}
