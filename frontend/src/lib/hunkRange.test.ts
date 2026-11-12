import { describe, expect, it } from "vitest";
import { hunkRange } from "./hunkRange";

// #682: a hunk's header reads "Lines 12–19", not git's "@@ -10,6 +12,8 @@".
describe("hunkRange", () => {
  it("names the new file's lines, with both sides in the tooltip", () => {
    expect(hunkRange({ oldStart: 10, oldLines: 6, newStart: 12, newLines: 8 })).toEqual({
      label: "Lines 12–19",
      title: "Before: lines 10–15. After: lines 12–19.",
    });
  });

  it("says Line for a one-line side", () => {
    expect(hunkRange({ oldStart: 3, oldLines: 1, newStart: 3, newLines: 1 })).toEqual({
      label: "Line 3",
      title: "Before: line 3. After: line 3.",
    });
  });

  it("names the removed lines of a hunk that only removes", () => {
    expect(hunkRange({ oldStart: 4, oldLines: 3, newStart: 3, newLines: 0 })).toEqual({
      label: "Removed lines 4–6",
      title: "Before: lines 4–6. After: none.",
    });
    expect(hunkRange({ oldStart: 7, oldLines: 1, newStart: 6, newLines: 0 })).toEqual({
      label: "Removed line 7",
      title: "Before: line 7. After: none.",
    });
  });

  it("has no lines before for a new file", () => {
    expect(hunkRange({ oldStart: 0, oldLines: 0, newStart: 1, newLines: 5 })).toEqual({
      label: "Lines 1–5",
      title: "Before: none. After: lines 1–5.",
    });
  });
});
