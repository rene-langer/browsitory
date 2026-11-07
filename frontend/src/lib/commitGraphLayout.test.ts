import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import { assignLanes, isSquashableRange } from "./commitGraphLayout";

function commit(id: string, parentIds: string[]): GraphCommit {
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
  };
}

describe("assignLanes", () => {
  it("puts every commit in a linear history on lane 0", () => {
    const commits = [commit("C", ["B"]), commit("B", ["A"]), commit("A", [])];

    const layouts = assignLanes(commits);

    expect(layouts.map((l) => l.lane)).toEqual([0, 0, 0]);
  });

  it("opens a second lane for a fork and closes it once the fork tip is placed", () => {
    // F1 and M2 are both children of M1 (a fork); F1 is newer.
    const commits = [commit("F1", ["M1"]), commit("M2", ["M1"]), commit("M1", [])];

    const layouts = assignLanes(commits);

    expect(layouts[0]).toEqual({
      commitId: "F1",
      lane: 0,
      laneSegmentId: 0,
      parentConnections: [{ parentId: "M1", lane: 0, segmentId: 0 }],
      hiddenParentIds: [],
      passThroughLanes: [],
    });
    expect(layouts[1]).toEqual({
      commitId: "M2",
      lane: 1,
      laneSegmentId: 1,
      parentConnections: [{ parentId: "M1", lane: 0, segmentId: 0 }],
      hiddenParentIds: [],
      passThroughLanes: [{ lane: 0, segmentId: 0 }],
    });
    expect(layouts[2].lane).toBe(0);
  });

  it("connects a merge commit's two parents to two different lanes", () => {
    const commits = [commit("M2", ["M1", "F1"]), commit("F1", ["M1"]), commit("M1", [])];

    const layouts = assignLanes(commits);

    expect(layouts[0].lane).toBe(0);
    expect(layouts[0].parentConnections).toEqual([
      { parentId: "M1", lane: 0, segmentId: 0 },
      { parentId: "F1", lane: 1, segmentId: 1 },
    ]);
    // F1's own lane (1) closes right after it, since its parent M1 is already tracked in lane 0
    // (opened by M2's first-parent connection) — F1 doesn't need to keep waiting for anything.
    expect(layouts[1]).toEqual({
      commitId: "F1",
      lane: 1,
      laneSegmentId: 1,
      parentConnections: [{ parentId: "M1", lane: 0, segmentId: 0 }],
      hiddenParentIds: [],
      passThroughLanes: [
        { lane: 0, segmentId: 0 },
        { lane: 1, segmentId: 1 },
      ],
    });
  });

  it("gives an unrelated branch that reuses a freed lane a different segment id", () => {
    // F1 forks off Trunk1 and closes immediately (Trunk1 is already tracked by Top's lane).
    // Later, F2 forks off Trunk2 and lands on the same numeric lane (1) that F1 freed. F1 and
    // F2 are unrelated branches sharing a lane number, so they must get different segment ids —
    // otherwise hovering one would also highlight the other (and any row using that lane number
    // would render at full opacity for both).
    const commits = [
      commit("Top", ["Trunk1"]),
      commit("F1", ["Trunk1"]),
      commit("Trunk1", ["Trunk2"]),
      commit("F2", ["Trunk2"]),
      commit("Trunk2", []),
    ];

    const layouts = assignLanes(commits);
    const byId = (commitId: string) => layouts.find((l) => l.commitId === commitId)!;

    const f1 = byId("F1");
    const f2 = byId("F2");
    expect(f1.lane).toBe(1);
    expect(f2.lane).toBe(1);
    expect(f1.laneSegmentId).not.toBe(f2.laneSegmentId);
  });

  it("gives a merge parent that is not in the list no lane, only a hidden-parent entry", () => {
    // An isolated branch's first-parent line: M2 merged F3, which the walk left out.
    const commits = [commit("After", ["M2"]), commit("M2", ["M1", "F3"]), commit("M1", ["Base"]), commit("Base", [])];

    const layouts = assignLanes(commits);

    expect(layouts.map((l) => l.lane)).toEqual([0, 0, 0, 0]);
    expect(layouts[1].parentConnections).toEqual([{ parentId: "M1", lane: 0, segmentId: 0 }]);
    expect(layouts[1].hiddenParentIds).toEqual(["F3"]);
    // No lane runs on below the merge waiting for F3.
    expect(layouts[2].passThroughLanes).toEqual([{ lane: 0, segmentId: 0 }]);
    expect(layouts[3].passThroughLanes).toEqual([{ lane: 0, segmentId: 0 }]);
    expect(layouts.flatMap((l) => l.hiddenParentIds)).toEqual(["F3"]);
  });

  it("keeps a lane for a merge parent that is in the list", () => {
    const commits = [commit("M2", ["M1", "F1"]), commit("F1", ["M1"]), commit("M1", [])];

    const layouts = assignLanes(commits);

    expect(layouts[0].hiddenParentIds).toEqual([]);
    expect(layouts[0].parentConnections).toHaveLength(2);
  });

  it("still runs a first parent past the end of the list down as a lane", () => {
    // The page cut off below B: its first parent A isn't loaded yet, but B's line continues.
    const layouts = assignLanes([commit("C", ["B"]), commit("B", ["A"])]);

    expect(layouts[1].hiddenParentIds).toEqual([]);
    expect(layouts[1].parentConnections).toEqual([{ parentId: "A", lane: 0, segmentId: 0 }]);
  });
});

describe("isSquashableRange", () => {
  it("accepts a straight single-parent chain", () => {
    const commits = [commit("C", ["B"]), commit("B", ["A"]), commit("A", [])];

    expect(isSquashableRange(commits, 0, 2)).toBe(true);
  });

  it("rejects a range that crosses a merge commit", () => {
    const commits = [
      commit("M2", ["M1", "F1"]),
      commit("F1", ["M1"]),
      commit("M1", []),
    ];

    // M2 has two parents, so squashing it with F1 (its second parent, a different branch)
    // doesn't correspond to any single rebase-plan group.
    expect(isSquashableRange(commits, 0, 1)).toBe(false);
  });

  it("rejects a range whose middle commit is a fork point (two children)", () => {
    // F1 and M2 are both children of M1: M1 isn't F1's sole descendant's-only-parent chain
    // partner, since selecting F1 through M1 would silently drop M2's own history.
    const commits = [
      commit("F1", ["M1"]),
      commit("M2", ["M1"]),
      commit("M1", []),
    ];

    expect(isSquashableRange(commits, 0, 2)).toBe(false);
  });

  it("a single-commit range is always squashable (trivial)", () => {
    const commits = [commit("A", [])];

    expect(isSquashableRange(commits, 0, 0)).toBe(true);
  });
});
