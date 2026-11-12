import { useCallback, useState } from "react";

/**
 * The merge editor's layout (#875): the whole file in panes (ours | result | theirs, the
 * default) or one block per conflict (#720). Persisted in `localStorage`, per viewer, like the
 * diff's Split view (`useDiffViewOptions`).
 */
export type MergeEditorView = "panes" | "blocks";

const VIEW_KEY = "mergeEditor.view";

// Storage can be missing or throw (a blocked or private context): the layout then lasts as long
// as the editor does.
function load(): MergeEditorView {
  try {
    return localStorage.getItem(VIEW_KEY) === "blocks" ? "blocks" : "panes";
  } catch {
    return "panes";
  }
}

export function useMergeEditorView(): [MergeEditorView, (view: MergeEditorView) => void] {
  const [view, setViewState] = useState(load);
  const setView = useCallback((next: MergeEditorView) => {
    setViewState(next);
    try {
      localStorage.setItem(VIEW_KEY, next);
    } catch {
      // Not persisted; the in-memory value still applies.
    }
  }, []);
  return [view, setView];
}
