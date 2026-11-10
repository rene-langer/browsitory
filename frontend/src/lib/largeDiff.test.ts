import { describe, expect, it } from "vitest";
import type { DiffHunk } from "../ipc/RepoClient";
import { LARGE_DIFF_LINES, collapseReason, diffLineCount, isGeneratedPath, windowHunks } from "./largeDiff";

const hunk = (count: number): DiffHunk => ({
  oldStart: 1, oldLines: count, newStart: 1, newLines: count,
  lines: Array.from({ length: count }, (_, i) => ({ origin: "Context" as const, content: `${i}\n` })),
});

describe("isGeneratedPath", () => {
  it.each(["Cargo.lock", "a/b/package-lock.json", "pnpm-lock.yaml", "go.sum", "dist/app.min.js", "x/STYLE.MIN.CSS", "app.js.map"])(
    "treats %s as generated",
    (path) => expect(isGeneratedPath(path)).toBe(true),
  );

  it.each(["Cargo.toml", "src/lock.rs", "package.json", "cargo.lock.md", "min.js", "main.js", "map"])(
    "leaves %s alone",
    (path) => expect(isGeneratedPath(path)).toBe(false),
  );
});

describe("collapseReason", () => {
  it("collapses generated files and diffs over the limit", () => {
    expect(collapseReason("yarn.lock", 1)).toBe("generated");
    expect(collapseReason("a.txt", LARGE_DIFF_LINES)).toBeNull();
    expect(collapseReason("a.txt", LARGE_DIFF_LINES + 1)).toBe("large");
    expect(collapseReason(undefined, LARGE_DIFF_LINES + 1)).toBe("large");
  });

  it("never collapses an empty diff", () => {
    expect(collapseReason("yarn.lock", 0)).toBeNull();
  });
});

describe("windowHunks", () => {
  it("returns the hunks themselves when they fit", () => {
    const hunks = [hunk(2), hunk(3)];
    expect(windowHunks(hunks, 5)).toBe(hunks);
  });

  it("cuts in the hunk the limit falls in and drops the rest", () => {
    const hunks = [hunk(2), hunk(3), hunk(4)];
    const out = windowHunks(hunks, 4);
    expect(out).toHaveLength(2);
    expect(out[0]).toBe(hunks[0]);
    expect(out[1].lines).toEqual(hunks[1].lines.slice(0, 2));
    expect(out[1].oldStart).toBe(hunks[1].oldStart);
    expect(diffLineCount(out)).toBe(4);
  });

  it("drops a hunk that starts exactly at the limit", () => {
    expect(windowHunks([hunk(2), hunk(3)], 2)).toHaveLength(1);
  });
});
