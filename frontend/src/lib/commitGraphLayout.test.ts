import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import { assignLanes, graphLaneCount, isSquashableRange, layoutLanes, type LaneLayout } from "./commitGraphLayout";

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

  it("stubs the first parent of an expanded merge's commit that is not in the list (#617)", () => {
    // M expanded in place: F1 forks from X, which another collapsed merge holds.
    const commits = [commit("M", ["A", "F1"]), commit("F1", ["X"]), commit("A", [])];

    const plain = assignLanes(commits);
    const expanded = assignLanes(commits, new Set(["F1"]));

    // Without the hint F1's lane would wait for X past every later row.
    expect(plain[2].passThroughLanes.map((entry) => entry.lane)).toEqual([0, 1]);
    expect(expanded[1].hiddenParentIds).toEqual(["X"]);
    expect(expanded[1].parentConnections).toEqual([]);
    expect(expanded[2].passThroughLanes.map((entry) => entry.lane)).toEqual([0]);
    expect(expanded.map((l) => l.lane)).toEqual([0, 1, 0]);
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

/** `count` commits in one line, newest first: "c<count-1>" down to the root "c0". */
function linearCommits(count: number): GraphCommit[] {
  return Array.from({ length: count }, (_, i) => {
    const n = count - 1 - i;
    return commit(`c${n}`, n === 0 ? [] : [`c${n - 1}`]);
  });
}

describe("graphLaneCount", () => {
  it("is one more than the highest lane a row, a pass-through or a parent connection uses", () => {
    expect(graphLaneCount([])).toBe(1);
    expect(graphLaneCount(assignLanes([commit("C", ["B"]), commit("B", ["A"]), commit("A", [])]))).toBe(1);
    // Rows on lane 0 only; lane 2 shows up only as a pass-through, lane 3 only as a connection.
    const row = (passThrough: number[], connections: number[]) => ({
      commitId: "x",
      lane: 0,
      laneSegmentId: 0,
      parentConnections: connections.map((lane) => ({ parentId: "p", lane, segmentId: 0 })),
      passThroughLanes: passThrough.map((lane) => ({ lane, segmentId: 0 })),
      hiddenParentIds: [],
    });
    expect(graphLaneCount([row([0, 2], [0])])).toBe(3);
    expect(graphLaneCount([row([0], [0]), row([1], [3])])).toBe(4);
  });

  // #836: the count spread every row's lanes into one `Math.max` call, which throws a RangeError
  // past the engine's argument limit (about 30,000 rows of a narrow history in V8), blanking the app.
  it("counts the lanes of 200,000 loaded rows", () => {
    const layouts = assignLanes(linearCommits(200_000));

    expect(graphLaneCount(layouts)).toBe(1);
  });
});

// A history in reverse-topological order (newest first), as `graph_log` returns it: commits are
// created oldest first, each on top of one or two existing ones (a fork, a merge, a new root now
// and then), and listed in reverse creation order. Seeded, so a failure reproduces.
function randomHistory(seed: number, count: number): GraphCommit[] {
  let state = seed;
  const random = () => {
    state = (state * 1_103_515_245 + 12_345) % 2_147_483_648;
    return state / 2_147_483_648;
  };
  const created: GraphCommit[] = [];
  let tips: string[] = [];
  for (let i = 0; i < count; i++) {
    const id = `s${seed}c${i}`;
    let parents: string[] = [];
    if (created.length > 0 && random() > 0.03) {
      const first = random() < 0.7 && tips.length > 0 ? tips[Math.floor(random() * tips.length)] : created[Math.floor(random() * created.length)].id;
      parents = [first];
      if (random() < 0.25) {
        const other = created[Math.max(0, created.length - 1 - Math.floor(random() * 40))].id;
        if (other !== first) parents.push(other);
      }
    }
    created.push(commit(id, parents));
    tips = tips.filter((tip) => !parents.includes(tip));
    tips.push(id);
    if (tips.length > 6) tips = tips.slice(-6);
  }
  return created.reverse();
}

/** Lays `commits` out page by page, as infinite scroll loads them, checking every prefix. */
function layOutInPages(commits: GraphCommit[], pageSize: number, merged?: ReadonlySet<string>): LaneLayout[] {
  const results: LaneLayout[] = [];
  let previous: LaneLayout | null = null;
  for (let end = pageSize; end < commits.length + pageSize; end += pageSize) {
    previous = layoutLanes(commits.slice(0, Math.min(end, commits.length)), merged, previous);
    results.push(previous);
  }
  return results;
}

describe("layoutLanes (#837)", () => {
  it("lays out each appended page to exactly what a full layout of the prefix gives", () => {
    // Every prefix of a short history, and pages of several sizes of longer ones.
    const cases: [number, number, number][] = [
      [1, 80, 1],
      [2, 80, 1],
      ...[3, 4, 5, 6].flatMap((seed) => [7, 50, 300].map((pageSize): [number, number, number] => [seed, 500, pageSize])),
    ];
    for (const [seed, length, pageSize] of cases) {
      const commits = randomHistory(seed, length);
      {
        const results = layOutInPages(commits, pageSize);
        results.forEach((result, page) => {
          const prefix = commits.slice(0, Math.min((page + 1) * pageSize, commits.length));
          const full = assignLanes(prefix);
          // As JSON: the same structure, compared far faster than a deep equal of every prefix.
          expect(JSON.stringify(result.layouts), `seed ${seed}, pages of ${pageSize}, page ${page}`).toBe(JSON.stringify(full));
          expect(result.laneCount).toBe(graphLaneCount(full));
          expect(result.indexById.get(prefix[prefix.length - 1].id)).toBe(prefix.length - 1);
          expect(result.indexById.size).toBe(prefix.length);
        });
      }
    }
  });

  it("matches the full layout with an expanded merge's commits too", () => {
    const commits = randomHistory(9, 400);
    const merged = new Set(commits.filter((_, index) => index % 13 === 5).map((c) => c.id));
    layOutInPages(commits, 37, merged).forEach((result, page) => {
      const prefix = commits.slice(0, Math.min((page + 1) * 37, commits.length));
      expect(result.layouts).toEqual(assignLanes(prefix, merged));
    });
  });

  it("lays out only the appended page when no earlier row waits for its commits", () => {
    const count = 3_000;
    const commits = Array.from({ length: count }, (_, i) => commit(`c${count - i}`, i + 1 < count ? [`c${count - i - 1}`] : []));
    const results = layOutInPages(commits, 300);
    expect(results.map((result) => result.rowsLaidOut)).toEqual(Array.from({ length: 10 }, () => 300));
  });

  it("lays out again from shortly before a row whose merge parent the new page brings", () => {
    // 1,000 rows of a main line; row 290 merges a side commit that only the second page lists.
    const count = 1_000;
    const ids = Array.from({ length: count }, (_, i) => `m${count - i}`);
    const commits = ids.map((id, i) => commit(id, i + 1 < count ? [ids[i + 1]] : []));
    commits[290] = commit(ids[290], [ids[291], "side"]);
    commits.splice(310, 0, commit("side", [ids[400]]));
    const [first, second] = layOutInPages(commits, 300);
    expect(first.layouts[290].hiddenParentIds).toEqual(["side"]);
    expect(second.layouts).toEqual(assignLanes(commits.slice(0, 600)));
    expect(second.layouts[290].hiddenParentIds).toEqual([]);
    // From the checkpoint at or before row 290, not from row 0.
    expect(second.rowsLaidOut).toBeLessThan(600);
    expect(second.rowsLaidOut).toBeGreaterThanOrEqual(310);
  });

  it("lays out from scratch when the list isn't an extension of the previous one", () => {
    const commits = randomHistory(11, 200);
    const previous = layoutLanes(commits.slice(0, 100), undefined, null);
    const other = randomHistory(12, 150);
    const result = layoutLanes(other, undefined, previous);
    expect(result.layouts).toEqual(assignLanes(other));
    expect(result.rowsLaidOut).toBe(150);
    // A different expanded-merge set also starts over.
    const again = layoutLanes(commits.slice(0, 150), new Set([commits[3].id]), layoutLanes(commits.slice(0, 100), undefined, null));
    expect(again.layouts).toEqual(assignLanes(commits.slice(0, 150), new Set([commits[3].id])));
    expect(again.rowsLaidOut).toBe(150);
  });

  it("leaves the previous result as it was", () => {
    const commits = randomHistory(13, 300);
    const first = layoutLanes(commits.slice(0, 100), undefined, null);
    const snapshot = JSON.stringify(first.layouts);
    const firstLength = first.layouts.length;
    layoutLanes(commits.slice(0, 300), undefined, first);
    expect(first.layouts).toHaveLength(firstLength);
    expect(JSON.stringify(first.layouts)).toBe(snapshot);
    expect(first.laneCount).toBe(graphLaneCount(assignLanes(commits.slice(0, 100))));
  });

  it("shares one pass-through list between consecutive rows whose lanes don't change", () => {
    // A side branch running beside the main line for 50 rows: every main-line row between its
    // tip and its fork point passes the same lane through.
    const main = Array.from({ length: 60 }, (_, i) => `m${60 - i}`);
    const commits = [commit("side", [main[55]]), ...main.map((id, i) => commit(id, i + 1 < main.length ? [main[i + 1]] : []))];
    const { layouts } = layoutLanes(commits, undefined, null);
    expect(layouts[2].passThroughLanes).toEqual(layouts[3].passThroughLanes);
    expect(layouts[2].passThroughLanes.length).toBeGreaterThan(0);
    expect(layouts[3].passThroughLanes).toBe(layouts[2].passThroughLanes);
    expect(layouts[40].passThroughLanes).toBe(layouts[2].passThroughLanes);
  });
});
