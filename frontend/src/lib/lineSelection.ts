import type { DiffHunk } from "../ipc/RepoClient";

export interface LineSelection {
  hunkIndex: number | null;
  anchor: number | null; // line index the range extends from
  cursor: number | null; // keyboard cursor line index
  selected: ReadonlySet<number>;
}

export const EMPTY_SELECTION: LineSelection = {
  hunkIndex: null,
  anchor: null,
  cursor: null,
  selected: new Set(),
};

export type SelectionAction =
  | { type: "click"; hunkIndex: number; line: number }
  | { type: "shiftClick"; hunkIndex: number; line: number }
  | { type: "toggle"; hunkIndex: number; line: number }
  | { type: "dragTo"; hunkIndex: number; line: number }
  | { type: "moveCursor"; delta: 1 | -1 }
  | { type: "extend"; delta: 1 | -1 }
  | { type: "focusHunk"; hunkIndex: number }
  | { type: "clear" };

// Add/Remove lines are selectable. Context lines, including EOF markers, are not.
export function isSelectable(hunks: readonly DiffHunk[], hunkIndex: number, line: number): boolean {
  const l = hunks[hunkIndex]?.lines[line];
  return l !== undefined && (l.origin === "Add" || l.origin === "Remove");
}

function selectableRange(
  hunks: readonly DiffHunk[],
  hunkIndex: number,
  from: number,
  to: number,
): Set<number> {
  const lo = Math.min(from, to);
  const hi = Math.max(from, to);
  const out = new Set<number>();
  for (let i = lo; i <= hi; i++) {
    if (isSelectable(hunks, hunkIndex, i)) out.add(i);
  }
  return out;
}

// Next selectable line from `from` in direction `delta`, or `from` if none exists.
function step(
  hunks: readonly DiffHunk[],
  hunkIndex: number,
  from: number | null,
  delta: 1 | -1,
): number | null {
  const count = hunks[hunkIndex]?.lines.length ?? 0;
  let i = from ?? (delta === 1 ? -1 : count);
  for (i += delta; i >= 0 && i < count; i += delta) {
    if (isSelectable(hunks, hunkIndex, i)) return i;
  }
  return from;
}

export function reduceSelection(
  state: LineSelection,
  action: SelectionAction,
  hunks: readonly DiffHunk[],
): LineSelection {
  switch (action.type) {
    case "clear":
      return EMPTY_SELECTION;

    case "focusHunk":
      return { ...EMPTY_SELECTION, hunkIndex: action.hunkIndex };

    case "click": {
      if (!isSelectable(hunks, action.hunkIndex, action.line)) {
        return { ...EMPTY_SELECTION, hunkIndex: action.hunkIndex };
      }
      return {
        hunkIndex: action.hunkIndex,
        anchor: action.line,
        cursor: action.line,
        selected: new Set([action.line]),
      };
    }

    case "shiftClick":
    case "dragTo": {
      const anchor =
        state.hunkIndex === action.hunkIndex && state.anchor !== null ? state.anchor : action.line;
      return {
        hunkIndex: action.hunkIndex,
        anchor,
        cursor: action.line,
        selected: selectableRange(hunks, action.hunkIndex, anchor, action.line),
      };
    }

    case "toggle": {
      if (!isSelectable(hunks, action.hunkIndex, action.line)) return state;
      const selected =
        state.hunkIndex === action.hunkIndex ? new Set(state.selected) : new Set<number>();
      if (selected.has(action.line)) selected.delete(action.line);
      else selected.add(action.line);
      return { hunkIndex: action.hunkIndex, anchor: action.line, cursor: action.line, selected };
    }

    case "moveCursor": {
      if (state.hunkIndex === null) return state;
      const cursor = step(hunks, state.hunkIndex, state.cursor, action.delta);
      return { hunkIndex: state.hunkIndex, anchor: cursor, cursor, selected: new Set() };
    }

    case "extend": {
      if (state.hunkIndex === null) return state;
      const hunkIndex = state.hunkIndex;
      const cursor = step(hunks, hunkIndex, state.cursor, action.delta);
      if (cursor === null) return state;
      const anchor = state.anchor ?? cursor;
      return {
        hunkIndex,
        anchor,
        cursor,
        selected: selectableRange(hunks, hunkIndex, anchor, cursor),
      };
    }
  }
}
