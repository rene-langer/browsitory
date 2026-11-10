import { describe, expect, it } from "vitest";
import type { SelectedRow } from "../state/useAppState";
import {
  initialNavigationHistory,
  moveToEntry,
  NAVIGATION_HISTORY_LIMIT,
  navigationTarget,
  recordVisit,
  type NavigationHistory,
  type VisitKind,
} from "./navigationHistory";

const c = (commitId: string): SelectedRow => ({ commitId });
const ids = (history: NavigationHistory) =>
  history.entries.map((row) => (row === "uncommitted" ? "U" : row.commitId)).join(" ");
const visit = (history: NavigationHistory, ...steps: [string, VisitKind][]) =>
  steps.reduce((h, [id, kind]) => recordVisit(h, id === "U" ? "uncommitted" : c(id), kind), history);
const always = () => true;

describe("navigation history (#618)", () => {
  it("starts on the given row and appends each jump", () => {
    const history = visit(initialNavigationHistory("uncommitted"), ["A", "jump"], ["B", "jump"]);
    expect(ids(history)).toBe("U A B");
    expect(history.index).toBe(2);
  });

  it("keeps the same object when the current row is selected again", () => {
    const history = visit(initialNavigationHistory("uncommitted"), ["A", "jump"]);
    expect(recordVisit(history, c("A"), "jump")).toBe(history);
  });

  it("merges consecutive keyboard moves into one entry", () => {
    const history = visit(initialNavigationHistory("uncommitted"), ["A", "jump"], ["B", "step"], ["C", "step"], ["D", "step"]);
    expect(ids(history)).toBe("U A D");
    // A jump ends the run: the next keyboard move adds an entry again.
    expect(ids(visit(history, ["X", "jump"], ["Y", "step"]))).toBe("U A D X Y");
  });

  it("drops a keyboard run that came back to where it started", () => {
    const history = visit(initialNavigationHistory("uncommitted"), ["A", "jump"], ["B", "step"], ["A", "step"]);
    expect(ids(history)).toBe("U A");
    expect(history.merging).toBe(false);
    expect(ids(visit(history, ["B", "step"]))).toBe("U A B");
  });

  it("goes back and forward, and a new visit drops the entries ahead", () => {
    const history = visit(initialNavigationHistory("uncommitted"), ["A", "jump"], ["B", "jump"], ["C", "jump"]);
    const back = navigationTarget(history, -1, always);
    expect(back).toBe(2);
    const onB = moveToEntry(history, back!);
    expect(navigationTarget(onB, -1, always)).toBe(1);
    expect(navigationTarget(onB, 1, always)).toBe(3);
    const branched = recordVisit(onB, c("X"), "jump");
    expect(ids(branched)).toBe("U A B X");
    expect(navigationTarget(branched, 1, always)).toBeNull();
  });

  it("is at its ends with nothing before or after", () => {
    const history = initialNavigationHistory("uncommitted");
    expect(navigationTarget(history, -1, always)).toBeNull();
    expect(navigationTarget(history, 1, always)).toBeNull();
  });

  it("skips an entry whose commit is no longer shown", () => {
    const history = visit(initialNavigationHistory("uncommitted"), ["A", "jump"], ["GONE", "jump"], ["C", "jump"]);
    const shown = (row: SelectedRow) => row === "uncommitted" || row.commitId !== "GONE";
    expect(navigationTarget(history, -1, shown)).toBe(1);
    expect(navigationTarget(moveToEntry(history, 1), 1, shown)).toBe(3);
    const nothingShown = () => false;
    expect(navigationTarget(history, -1, nothingShown)).toBeNull();
  });

  it("keeps only the newest entries", () => {
    let history = initialNavigationHistory("uncommitted");
    for (let i = 0; i < NAVIGATION_HISTORY_LIMIT + 5; i++) history = recordVisit(history, c(`c${i}`), "jump");
    expect(history.entries).toHaveLength(NAVIGATION_HISTORY_LIMIT);
    expect(history.index).toBe(NAVIGATION_HISTORY_LIMIT - 1);
    expect(history.entries[0]).toEqual(c("c5"));
  });
});
