import { afterEach, describe, expect, it } from "vitest";
import { lineRangeLabel, selectedLineSpan } from "./selectedLines";

function container(lines: Array<number | null>): HTMLElement {
  const root = document.createElement("div");
  for (const [i, n] of lines.entries()) {
    const row = document.createElement("div");
    if (n !== null) row.dataset.lineNumber = String(n);
    row.textContent = `row ${i}`;
    root.append(row);
  }
  document.body.append(root);
  return root;
}

function select(startNode: Node, startOffset: number, endNode: Node, endOffset: number) {
  const range = document.createRange();
  range.setStart(startNode, startOffset);
  range.setEnd(endNode, endOffset);
  const selection = window.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
}

afterEach(() => {
  window.getSelection()?.removeAllRanges();
  document.body.innerHTML = "";
});

describe("selectedLineSpan (#447)", () => {
  it("spans the numbered rows the selection touches, skipping unnumbered ones", () => {
    const root = container([7, null, 8, 9]);
    select(root.children[0].firstChild!, 2, root.children[2].firstChild!, 3);
    expect(selectedLineSpan(root)).toEqual({ start: 7, end: 8 });
  });

  it("ignores a row the selection only reaches the start of", () => {
    const root = container([1, 2, 3]);
    // As after a triple-click on row 1: the selection ends at the start of row 2.
    select(root.children[0].firstChild!, 0, root.children[1].firstChild!, 0);
    expect(selectedLineSpan(root)).toEqual({ start: 1, end: 1 });
  });

  it("ignores a row the selection only leaves at its end", () => {
    const root = container([1, 2, 3]);
    const first = root.children[0].firstChild!;
    select(first, first.textContent!.length, root.children[2].firstChild!, 2);
    expect(selectedLineSpan(root)).toEqual({ start: 2, end: 3 });
  });

  it("is null without a selection, outside the container, or on unnumbered rows only", () => {
    const root = container([null, 4]);
    const other = container([5]);
    expect(selectedLineSpan(root)).toBeNull();
    select(other.children[0].firstChild!, 0, other.children[0].firstChild!, 3);
    expect(selectedLineSpan(root)).toBeNull();
    select(root.children[0].firstChild!, 0, root.children[0].firstChild!, 3);
    expect(selectedLineSpan(root)).toBeNull();
    expect(selectedLineSpan(null)).toBeNull();
  });

  it("labels one line or several", () => {
    expect(lineRangeLabel(3, 3)).toBe("line 3");
    expect(lineRangeLabel(3, 5)).toBe("lines 3–5");
  });
});
