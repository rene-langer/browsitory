import { describe, expect, it } from "vitest";
import { wordDiff } from "./wordDiff";

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

  it("returns no segments for two empty strings", () => {
    const result = wordDiff("", "");
    expect(result.oldSegments).toEqual([]);
    expect(result.newSegments).toEqual([]);
  });
});
