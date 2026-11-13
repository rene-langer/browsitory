import { describe, expect, it } from "vitest";
import { blockWordMarks, MAX_BLOCK_LINE_CELLS } from "./conflictWordDiff";

// #879: the words that differ between a block's ours and theirs lines.

/** Each line's changed words, or null for a line without marks. */
function changed(marks: ReturnType<typeof blockWordMarks>["ours"]): (string[] | null)[] {
  return marks.map((line) => (line === undefined ? null : line.filter((part) => part.changed).map((part) => part.text)));
}

describe("blockWordMarks", () => {
  it("marks the words that differ in a pair of lines, and keeps the line's text", () => {
    const marks = blockWordMarks("let total = 1;\n", "let total = 2;\n");
    expect(changed(marks.ours)).toEqual([["1"]]);
    expect(changed(marks.theirs)).toEqual([["2"]]);
    expect(marks.ours[0]?.map((part) => part.text).join("")).toBe("let total = 1;");
    expect(marks.theirs[0]?.map((part) => part.text).join("")).toBe("let total = 2;");
  });

  it("leaves the lines both sides share, and a line only one side has, unmarked", () => {
    const marks = blockWordMarks("let total = 1;\nsame\n", "let total = 2;\nsame\nextra\n");
    expect(changed(marks.ours)).toEqual([["1"], null]);
    expect(changed(marks.theirs)).toEqual([["2"], null, null]);
  });

  it("pairs the lines around the ones both sides share, not by position", () => {
    const marks = blockWordMarks("a = 1\nb = 2\n", "# note\na = 1\nb = 3\n");
    expect(changed(marks.ours)).toEqual([null, ["2"]]);
    expect(changed(marks.theirs)).toEqual([null, null, ["3"]]);
  });

  it("leaves a pair with no word in common unmarked: the whole line differs", () => {
    const marks = blockWordMarks("o1\n", "t1\n");
    expect(changed(marks.ours)).toEqual([null]);
    expect(changed(marks.theirs)).toEqual([null]);
  });

  it("has an entry per line, also for an empty side and for lines ending in CRLF", () => {
    expect(blockWordMarks("", "a\n")).toEqual({ ours: [], theirs: [undefined] });
    const marks = blockWordMarks("x = 1\r\n", "x = 2\r\n");
    expect(changed(marks.ours)).toEqual([["1"]]);
    expect(marks.ours[0]?.map((part) => part.text).join("")).toBe("x = 1");
  });

  it("pairs by position when the block is too long to match its lines", () => {
    const count = Math.ceil(Math.sqrt(MAX_BLOCK_LINE_CELLS)) + 1;
    const ours = Array.from({ length: count }, (_, i) => `v${i} = 1\n`).join("");
    const theirs = Array.from({ length: count }, (_, i) => `v${i} = 2\n`).join("");
    const marks = blockWordMarks(ours, theirs);
    expect(marks.ours).toHaveLength(count);
    expect(changed(marks.ours)[count - 1]).toEqual(["1"]);
    expect(changed(marks.theirs)[0]).toEqual(["2"]);
  });
});
