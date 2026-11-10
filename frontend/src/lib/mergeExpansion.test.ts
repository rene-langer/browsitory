import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import { assignLanes } from "./commitGraphLayout";
import { expandedFrom, expansionHasMore, withExpandedMerges } from "./mergeExpansion";

function commit(id: string, parentIds: string[], extra: Partial<GraphCommit> = {}): GraphCommit {
  return {
    id,
    shortId: id,
    summary: id,
    authorName: "Test",
    authorEmail: "test@example.com",
    timestamp: 0,
    parentIds,
    branchRefs: [],
    remoteBranchRefs: [],
    ...extra,
  };
}

// trunk's first-parent line: After → M (merged F3) → Direct → Base; the side branch F1..F3 forks
// from Base.
const rows = [
  commit("After", ["M"]),
  commit("M", ["Direct", "F3"], { mergedCount: 3 }),
  commit("Direct", ["Base"]),
  commit("Base", []),
];
const side = [commit("F3", ["F2"]), commit("F2", ["F1"]), commit("F1", ["Base"])];

describe("withExpandedMerges", () => {
  it("returns the rows unchanged without an expanded merge", () => {
    expect(withExpandedMerges(rows, {})).toBe(rows);
  });

  it("lists an expanded merge's commits right below its row", () => {
    const shown = withExpandedMerges(rows, { M: { commits: side, limit: 100 } });

    expect(shown.map((c) => c.id)).toEqual(["After", "M", "F3", "F2", "F1", "Direct", "Base"]);
  });

  it("does not repeat a commit that is already a row or listed by an earlier merge", () => {
    const shown = withExpandedMerges(rows, { M: { commits: [...side, commit("Direct", ["Base"])], limit: 100 } });

    expect(shown.map((c) => c.id)).toEqual(["After", "M", "F3", "F2", "F1", "Direct", "Base"]);
  });

  it("ignores an expansion whose merge is no row", () => {
    expect(withExpandedMerges(rows, { Gone: { commits: side, limit: 100 } }).map((c) => c.id)).toEqual(
      rows.map((c) => c.id),
    );
  });
});

describe("expandedFrom", () => {
  it("maps the spliced commits to their merge, not a commit that is a row of its own", () => {
    const expansions = { M: { commits: [...side, commit("Direct", ["Base"])], limit: 100 } };
    const owners = expandedFrom(withExpandedMerges(rows, expansions), expansions);

    expect(Object.fromEntries(owners)).toEqual({ F3: "M", F2: "M", F1: "M", Direct: "M" });
  });

  it("stops at the first row after the merge's run", () => {
    const expansions = { M: { commits: side, limit: 100 } };
    const shown = withExpandedMerges(rows, expansions);

    expect([...expandedFrom(shown, expansions).keys()]).toEqual(["F3", "F2", "F1"]);
  });
});

describe("expansionHasMore", () => {
  it("is true while the merge filled its limit", () => {
    expect(expansionHasMore({ commits: side, limit: 3 })).toBe(true);
    expect(expansionHasMore({ commits: side, limit: 100 })).toBe(false);
  });
});

describe("lanes of an expanded merge (#617)", () => {
  it("lays an expanded merge out as the full graph of the same commits", () => {
    const expansions = { M: { commits: side, limit: 100 } };
    const shown = withExpandedMerges(rows, expansions);
    const merged = new Set(expandedFrom(shown, expansions).keys());
    // What walking every ancestor lists for the same history.
    const full = [rows[0], rows[1], ...side, rows[2], rows[3]];

    const layouts = assignLanes(shown, merged);
    const lane = (id: string) => layouts.find((l) => l.commitId === id)!.lane;

    expect(layouts).toEqual(assignLanes(full));
    expect(["After", "M", "Direct"].map(lane)).toEqual([0, 0, 0]);
    expect(["F3", "F2", "F1"].map(lane)).toEqual([1, 1, 1]);
    // No stub: the merge's second parent is listed, and the side branch ends at its fork point.
    expect(layouts.flatMap((l) => l.hiddenParentIds)).toEqual([]);
    const f1 = layouts.find((l) => l.commitId === "F1")!;
    expect(f1.parentConnections).toEqual([{ parentId: "Base", lane: 1, segmentId: expect.any(Number) }]);
  });

  it("matches the collapsed layout once the merge collapses again", () => {
    const collapsed = assignLanes(rows);
    const expansions = { M: { commits: side, limit: 100 } };
    const shown = withExpandedMerges(rows, expansions);
    const expanded = assignLanes(shown, new Set(expandedFrom(shown, expansions).keys()));

    expect(collapsed[1].hiddenParentIds).toEqual(["F3"]);
    expect(expanded.find((l) => l.commitId === "M")!.hiddenParentIds).toEqual([]);
    expect(assignLanes(withExpandedMerges(rows, {}))).toEqual(collapsed);
  });

  it("stubs the first parent of a merged commit that forks from a commit not in the list", () => {
    // F1 forks from X, a commit another (collapsed) merge holds.
    const forked = [commit("F3", ["F2"]), commit("F2", ["F1"]), commit("F1", ["X"])];
    const expansions = { M: { commits: forked, limit: 100 } };
    const shown = withExpandedMerges(rows, expansions);

    const layouts = assignLanes(shown, new Set(expandedFrom(shown, expansions).keys()));

    const f1 = layouts.find((l) => l.commitId === "F1")!;
    expect(f1.hiddenParentIds).toEqual(["X"]);
    expect(f1.parentConnections).toEqual([]);
    // Its lane ends there instead of running past Direct and Base.
    expect(layouts.find((l) => l.commitId === "Base")!.passThroughLanes.map((entry) => entry.lane)).toEqual([0]);
  });
});
