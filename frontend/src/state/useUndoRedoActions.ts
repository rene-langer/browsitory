import { useCallback, useEffect, useRef } from "react";
import type { UseAppStateResult } from "./useAppState";
import type { ToastAction } from "./useToasts";

/**
 * `RepoWorkspace`'s Undo/Redo with their toasts, and the refresh of the undo state when the
 * window regains focus. `mounted` is true while the workspace is mounted.
 */
export function useUndoRedoActions(
  appState: UseAppStateResult,
  active: boolean,
  pushToast: (message: string, action?: ToastAction) => void,
) {
  const { undo: undoStep, redo: redoStep, refreshUndoState } = appState;
  const { pending, transfer } = appState.state;
  // The toast stack is app-level and outlives this workspace (tab switch unmounts it), so a
  // toast action checks it's still mounted and idle, read through refs at click time.
  const mounted = useRef(true);
  const busy = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
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
          label: kind === "undo" ? "Redo" : "Undo",
          run: () => {
            if (mounted.current && !busy.current) run(other);
          },
        });
      });
    },
    [undoStep, redoStep, pushToast],
  );
  const handleUndo = useCallback(() => runUndoStep("undo"), [runUndoStep]);
  const handleRedo = useCallback(() => runUndoStep("redo"), [runUndoStep]);

  useEffect(() => {
    if (!active) return;
    const onFocus = () => void refreshUndoState();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [active, refreshUndoState]);

  return { mounted, handleUndo, handleRedo };
}
