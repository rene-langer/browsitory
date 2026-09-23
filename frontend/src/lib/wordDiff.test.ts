import { describe, expect, it } from "vitest";
import { MAX_LCS_CELLS, wordDiff } from "./wordDiff";

describe("wordDiff", () => {
  it("marks only the changed words between two similar lines", () => {
    const result = wordDiff("const foo = 1;", "const foo = 2;");
    expect(result.oldSegments).toEqual([
      { text: "const foo = ", changed: false },
      { text: "1", changed: true },
      { text: ";", changed: false },
    ]);
    expect(result.newSegments).toEqual([
      { text: "const foo = ", changed: false },
      { text: "2", changed: true },
      { text: ";", changed: false },
    ]);
  });

  it("marks the whole line changed when there is no common structure", () => {
    const result = wordDiff("abc", "xyz");
    expect(result.oldSegments).toEqual([{ text: "abc", changed: true }]);
    expect(result.newSegments).toEqual([{ text: "xyz", changed: true }]);
  });

  it("reconstructs the original lines exactly by joining segment text", () => {
    const oldText = "  const foo = bar(1, 2);  // note";
    const newText = "  const foo = baz(1, 3);  // updated";
    const result = wordDiff(oldText, newText);
    expect(result.oldSegments.map((s) => s.text).join("")).toBe(oldText);
    expect(result.newSegments.map((s) => s.text).join("")).toBe(newText);
  });

  it("falls back to marking both lines wholly changed once the LCS table would exceed MAX_LCS_CELLS", () => {
    // A minified/lockfile-style line: thousands of tokens, differing only at the very end. Below
    // the cap this would get a precise one-word highlight; above it, the table is never built.
    const words = Array.from({ length: 1000 }, (_, k) => `w${k}`);
    const oldText = `${words.join(" ")} old`;
    const newText = `${words.join(" ")} new`;
    expect(oldText.match(/\w+|\s+|[^\w\s]/g)!.length ** 2).toBeGreaterThan(MAX_LCS_CELLS);

    const result = wordDiff(oldText, newText);

    expect(result.oldSegments).toEqual([{ text: oldText, changed: true }]);
    expect(result.newSegments).toEqual([{ text: newText, changed: true }]);
  });

  it("still diffs word by word at exactly the cap", () => {
    // 100 words + 99 separating spaces + 1 punctuation token = 200 tokens a side: 200 × 200 is
    // exactly MAX_LCS_CELLS, which is still allowed.
    const words = Array.from({ length: 100 }, (_, k) => `w${k}`).join(" ");
    expect(200 * 200).toBe(MAX_LCS_CELLS);

    const result = wordDiff(`${words}!`, `${words}?`);

    expect(result.newSegments).toEqual([
      { text: words, changed: false },
      { text: "?", changed: true },
    ]);
  });

  it("returns no segments for two empty strings", () => {
    const result = wordDiff("", "");
    expect(result.oldSegments).toEqual([]);
    expect(result.newSegments).toEqual([]);
  });
});
