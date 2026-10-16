import { describe, expect, it } from "vitest";
import type { DiffHunk } from "../ipc/RepoClient";
import { EMPTY_SELECTION, reduceSelection, type LineSelection, type SelectionAction } from "./lineSelection";

const hunk = (origins: Array<"Add" | "Remove" | "Context">): DiffHunk => ({
  oldStart: 1, oldLines: 0, newStart: 1, newLines: 0,
  lines: origins.map((origin, i) => ({ origin, content: `l${i}\n` })),
});
// indices: 0 ctx, 1 rem, 2 add, 3 ctx, 4 add
const hunks = [hunk(["Context", "Remove", "Add", "Context", "Add"]), hunk(["Add", "Add"])];
const run = (actions: SelectionAction[], start: LineSelection = EMPTY_SELECTION) =>
  actions.reduce((s, a) => reduceSelection(s, a, hunks), start);
const sel = (s: LineSelection) => [...s.selected].sort((a, b) => a - b);

describe("reduceSelection", () => {
  it("click selects one changed line and ignores context", () => {
    expect(sel(run([{ type: "click", hunkIndex: 0, line: 2 }]))).toEqual([2]);
    expect(sel(run([{ type: "click", hunkIndex: 0, line: 0 }]))).toEqual([]);
  });

  it("shift-click selects the changed lines in the range", () => {
    const s = run([{ type: "click", hunkIndex: 0, line: 1 }, { type: "shiftClick", hunkIndex: 0, line: 4 }]);
    expect(sel(s)).toEqual([1, 2, 4]);
  });

  it("toggle adds and removes single lines", () => {
    const s = run([
      { type: "click", hunkIndex: 0, line: 1 },
      { type: "toggle", hunkIndex: 0, line: 4 },
      { type: "toggle", hunkIndex: 0, line: 1 },
    ]);
    expect(sel(s)).toEqual([4]);
  });

  it("selecting in another hunk replaces the selection", () => {
    const s = run([{ type: "click", hunkIndex: 0, line: 1 }, { type: "toggle", hunkIndex: 1, line: 0 }]);
    expect(s.hunkIndex).toBe(1);
    expect(sel(s)).toEqual([0]);
  });

  it("dragTo behaves like shift-click from the anchor", () => {
    const s = run([{ type: "click", hunkIndex: 0, line: 4 }, { type: "dragTo", hunkIndex: 0, line: 2 }]);
    expect(sel(s)).toEqual([2, 4]);
  });

  it("keyboard cursor skips context lines and extend grows the selection", () => {
    let s = run([{ type: "focusHunk", hunkIndex: 0 }, { type: "moveCursor", delta: 1 }]);
    expect(s.cursor).toBe(1);
    s = run([{ type: "extend", delta: 1 }, { type: "extend", delta: 1 }], s);
    expect(sel(s)).toEqual([1, 2, 4]);
    expect(s.cursor).toBe(4);
  });

  it("moveCursor clears the selection and stops at the ends", () => {
    let s = run([{ type: "click", hunkIndex: 0, line: 4 }, { type: "moveCursor", delta: 1 }]);
    expect(s.cursor).toBe(4);
    expect(sel(s)).toEqual([]);
    s = run([{ type: "moveCursor", delta: -1 }, { type: "moveCursor", delta: -1 }, { type: "moveCursor", delta: -1 }], s);
    expect(s.cursor).toBe(1);
  });

  it("never selects an EOF marker", () => {
    const eof: DiffHunk[] = [{ oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [
      { origin: "Add", content: "x" },
      { origin: "Context", content: "\n\\ No newline at end of file\n" },
    ] }];
    const s = reduceSelection(EMPTY_SELECTION, { type: "shiftClick", hunkIndex: 0, line: 1 }, eof);
    expect([...s.selected]).toEqual([]);
  });

  it("clear resets everything", () => {
    expect(run([{ type: "click", hunkIndex: 0, line: 1 }, { type: "clear" }])).toEqual(EMPTY_SELECTION);
  });
});
