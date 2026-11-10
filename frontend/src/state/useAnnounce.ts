import { useCallback, useEffect, useRef } from "react";
import type { UseAppStateResult } from "./useAppState";
import type { PushToast, ToastAction } from "./useToasts";

export interface AnnounceOptions {
  /**
   * The operation records an undo step (`docs/guide/undo-redo.md`), so its toast offers Undo
   * (#662). Leave it off for anything Undo can't bring back: transfers, stashes, remotes.
   */
  undoable?: boolean;
}

/**
 * Wraps a mutation so its success shows `label` as a toast (`RepoWorkspace`'s `announce`).
 * `label` can be built from the call's arguments, so the toast names what changed.
 */
export function useAnnounce(
  appState: UseAppStateResult,
  pushToast: PushToast,
  undoToastAction: () => ToastAction,
) {
  // Positive feedback (AUD-2026-09-20-FB-003). The mutation runner swallows failures into
  // `state.error`, so success is detected as "pending fell back to false with no error" after a
  // labelled action; `announce` clears any stale error first so that check is unambiguous.
  const pending = useRef<{ label: string; undoable: boolean } | null>(null);
  const { pending: statePending, error: stateError } = appState.state;
  useEffect(() => {
    if (statePending || pending.current === null) return;
    const { label, undoable } = pending.current;
    pending.current = null;
    if (stateError === null) pushToast(label, undoable ? { action: undoToastAction() } : undefined);
  }, [statePending, stateError, pushToast, undoToastAction]);
  const { dismissError } = appState;
  const announce = useCallback(
    <A extends unknown[], R,>(label: string | ((...args: NoInfer<A>) => string), action: (...args: A) => R, options: AnnounceOptions = {}) =>
      (...args: A): R => {
        dismissError();
        pending.current = {
          label: typeof label === "string" ? label : label(...args),
          undoable: options.undoable === true,
        };
        return action(...args);
      },
    [dismissError],
  );
  return announce;
}
