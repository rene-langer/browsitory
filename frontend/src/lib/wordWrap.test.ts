import { describe, expect, it } from "vitest";
import type { DiffHunk } from "../ipc/RepoClient";
import { LONG_LINE_CHARS, wrapLines } from "./wordWrap";

const hunk = (content: string): DiffHunk => ({
  oldStart: 1,
  oldLines: 1,
  newStart: 1,
  newLines: 1,
  lines: [{ origin: "Context", content }],
});
const short = [hunk("let x = 1;\n")];

describe("wrapLines", () => {
  it("follows On and Off whatever the view", () => {
    expect(wrapLines("On", { split: false, path: "a.rs", hunks: short })).toBe(true);
    expect(wrapLines("Off", { split: true, path: "notes.md", hunks: short })).toBe(false);
  });

  it("Auto keeps code unwrapped in the unified view", () => {
    expect(wrapLines("Auto", { split: false, path: "src/main.rs", hunks: short })).toBe(false);
    expect(wrapLines("Auto", { split: false, path: undefined, hunks: null })).toBe(false);
  });

  it("Auto wraps the split view", () => {
    expect(wrapLines("Auto", { split: true, path: "src/main.rs", hunks: short })).toBe(true);
  });

  it("Auto wraps prose files", () => {
    for (const path of ["README.md", "docs/guide.MARKDOWN", "notes.txt", "a/b.rst", "book.adoc"]) {
      expect(wrapLines("Auto", { split: false, path, hunks: short }), path).toBe(true);
    }
  });

  it("Auto wraps a diff with a very long line (minified or generated files)", () => {
    const long = [hunk(`${"x".repeat(LONG_LINE_CHARS + 1)}\n`)];
    expect(wrapLines("Auto", { split: false, path: "dist/app.min.js", hunks: long })).toBe(true);
    const atLimit = [hunk(`${"x".repeat(LONG_LINE_CHARS)}\n`)];
    expect(wrapLines("Auto", { split: false, path: "dist/app.js", hunks: atLimit })).toBe(false);
  });
});
