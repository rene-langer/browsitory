import { useCallback, useEffect, useLayoutEffect, useReducer, useRef } from "react";
import type { UseAppStateResult } from "./useAppState";
import type { PushToast, ToastAction } from "./useToasts";

/** The info toast when a toast's Undo would now undo something newer than its own operation. */
export const STALE_UNDO_MESSAGE =
  "Not undone: newer changes came after this one. Use Undo in the toolbar to step back through them.";

/**
 * `RepoWorkspace`'s Undo/Redo with their toasts, the Undo a success toast offers
 * (`undoToastAction`), and the refresh of the undo state when the window regains focus.
 * `mounted` is true while the workspace is mounted.
 */
export function useUndoRedoActions(appState: UseAppStateResult, active: boolean, pushToast: PushToast) {
  const { undo: undoStep, redo: redoStep, refreshUndoState } = appState;
  const { pending, transfer } = appState.state;
  // The toast stack is app-level and outlives this workspace (a tab switch hides it, and later
  // unmounts it, #642), so a toast action checks it's still mounted, shown and idle, read
  // through refs at click time. The refs (and the offers' versions below) are set in layout
  // effects, which run before the commit that shows a toast ends: a passive effect runs later,
  // and a click in between found the toast's own Undo stale or busy (#769).
  const mounted = useRef(true);
  const shown = useRef(active);
  useLayoutEffect(() => {
    shown.current = active;
  }, [active]);
  const busy = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useLayoutEffect(() => {
    busy.current = pending || transfer !== null;
  }, [pending, transfer]);
  // Each success toast offers the opposite step.
  const runUndoStep = useCallback(
    function run(kind: "undo" | "redo") {
      const step = kind === "undo" ? undoStep : redoStep;
      void step().then((outcome) => {
        if (outcome?.kind !== "Done") return;
        const other = kind === "undo" ? "redo" : "undo";
        pushToast(`${kind === "undo" ? "Undid" : "Redid"} ${outcome.label}`, {
          action: {
            label: kind === "undo" ? "Redo" : "Undo",
            run: () => {
              if (mounted.current && shown.current && !busy.current) run(other);
            },
          },
        });
      });
    },
    [undoStep, redoStep, pushToast],
  );
  const handleUndo = useCallback(() => runUndoStep("undo"), [runUndoStep]);

  // A success toast's Undo undoes the newest step, so it must still be that toast's own step
  // (#662). `undoVersion` counts changes to the undo stack's labels; each offer records the count
  // in the first commit after it is made in which nothing is pending, so its operation's refresh
  // (and with it its undo step) is on screen, and its Undo runs only while the count is
  // unchanged. Repeating an identical step ("Stage lines in a.txt" twice) isn't counted; undoing
  // either one restores the same state.
  const { undo: undoLabel, redo: redoLabel } = appState.state.undoState;
  const undoVersion = useRef({ count: 0, undo: undoLabel, redo: redoLabel });
  const offers = useRef<{ version: number | null }[]>([]);
  const [, commitOffers] = useReducer((n: number) => n + 1, 0);
  useLayoutEffect(() => {
    const seen = undoVersion.current;
    if (seen.undo !== undoLabel || seen.redo !== redoLabel) {
      undoVersion.current = { count: seen.count + 1, undo: undoLabel, redo: redoLabel };
    }
  }, [undoLabel, redoLabel]);
  const settled = !pending && transfer === null;
  useLayoutEffect(() => {
    if (offers.current.length === 0 || !settled) return;
    for (const offer of offers.current) offer.version = undoVersion.current.count;
    offers.current = [];
  });
  // Read at click time, so `undoToastAction` stays stable: the outcome toasts' effects list it,
  // and a new identity would show their toast again.
  const latest = useRef({ pushToast, runUndoStep });
  useLayoutEffect(() => {
    latest.current = { pushToast, runUndoStep };
  }, [pushToast, runUndoStep]);
  /** The "Undo" for the toast of an operation that just recorded an undo step. */
  const undoToastAction = useCallback((): ToastAction => {
    const offer: { version: number | null } = { version: null };
    offers.current.push(offer);
    commitOffers();
    return {
      label: "Undo",
      run: () => {
        if (!mounted.current || !shown.current || busy.current) return;
        if (offer.version !== undoVersion.current.count) {
          latest.current.pushToast(STALE_UNDO_MESSAGE, { tone: "info" });
          return;
        }
        latest.current.runUndoStep("undo");
      },
    };
  }, []);
  const handleRedo = useCallback(() => runUndoStep("redo"), [runUndoStep]);

  useEffect(() => {
    if (!active) return;
    const onFocus = () => void refreshUndoState();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [active, refreshUndoState]);

  return { mounted, handleUndo, handleRedo, undoToastAction };
}
