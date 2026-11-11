import { createContext, useCallback, useContext, useEffect, useMemo, useRef, type PointerEvent } from "react";
import type { CommitDropRun, DraggedCommits } from "../lib/actions/commitActions";
import { BRANCH_DROP_SCOPE } from "../components/BranchTreeView";
import { usePointerDrag, type PointerDragOver, type PointerDragState } from "./usePointerDrag";

/** What the drop side (the Branches section) does with a commit drag. */
export interface CommitDropTarget {
  onStart: (dragged: DraggedCommits) => void;
  onCancel: () => void;
  /** Released over `over` (`null`: over no branch row or folder). */
  onDrop: (dragged: DraggedCommits, over: PointerDragOver | null) => void;
  /** The element to scroll while the pointer is near its top or bottom edge. */
  autoScroll?: () => HTMLElement | null;
}

/**
 * Commits dragged from the graph onto a branch in the sidebar (#744). The graph starts the drag;
 * the Branches section shows what a drop would do and opens its menu. This session, made by
 * `RepoWorkspace` and shared through `CommitDragContext`, connects the two and runs the choice.
 */
export interface CommitDragSession {
  drag: PointerDragState<DraggedCommits> | null;
  startDrag: (event: PointerEvent, dragged: DraggedCommits) => void;
  /** The Branches section registers as the drop side; the returned function unregisters it. */
  registerTarget: (target: CommitDropTarget) => () => void;
  /** Runs a drop menu's choice: the commit menu's Cherry-pick, the parent picker or a sequence. */
  run: (run: CommitDropRun) => void;
}

/** `null` outside a workspace: the graph's rows then don't drag. */
export const CommitDragContext = createContext<CommitDragSession | null>(null);

export function useCommitDrag(): CommitDragSession | null {
  return useContext(CommitDragContext);
}

/** The provider's side: one commit drag at a time, for `CommitDragContext`. */
export function useCommitDragSession(onRun: (run: CommitDropRun) => void): CommitDragSession {
  const targetRef = useRef<CommitDropTarget | null>(null);
  const onRunRef = useRef(onRun);
  useEffect(() => {
    onRunRef.current = onRun;
  });
  const { drag, startDrag } = usePointerDrag<DraggedCommits>({
    // The Branches section's rows and folder headers, the targets of its branch drags too.
    scope: BRANCH_DROP_SCOPE,
    autoScroll: () => targetRef.current?.autoScroll?.() ?? null,
    onStart: (dragged) => targetRef.current?.onStart(dragged),
    onCancel: () => targetRef.current?.onCancel(),
    onDrop: (dragged, over) => targetRef.current?.onDrop(dragged, over),
  });
  const registerTarget = useCallback((target: CommitDropTarget) => {
    targetRef.current = target;
    return () => {
      if (targetRef.current === target) targetRef.current = null;
    };
  }, []);
  const run = useCallback((choice: CommitDropRun) => onRunRef.current(choice), []);
  return useMemo(() => ({ drag, startDrag, registerTarget, run }), [drag, startDrag, registerTarget, run]);
}
