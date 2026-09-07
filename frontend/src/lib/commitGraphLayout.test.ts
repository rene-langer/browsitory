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
      passThroughLanes: [],
    });
    expect(layouts[1]).toEqual({
      commitId: "M2",
      lane: 1,
      laneSegmentId: 1,
      parentConnections: [{ parentId: "M1", lane: 0, segmentId: 0 }],
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
