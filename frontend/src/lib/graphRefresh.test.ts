import { describe, expect, it } from "vitest";
import type { GraphCommit, GraphPage } from "../ipc/RepoClient";
import { applyGraphRefresh } from "./graphRefresh";

const row = (id: string, branchRefs: string[] = []): GraphCommit => ({
  id,
  shortId: id,
  summary: id,
  authorName: "a",
  authorEmail: "a@b",
  timestamp: 0,
  parentIds: [],
  branchRefs,
  remoteBranchRefs: [],
  tags: [],
});
const page = (commits: GraphCommit[], refresh: GraphPage["refresh"]): GraphPage => ({
  commits,
  nextCursor: null,
  stale: false,
  walk: "w",
  badges: "b",
  refresh,
});

// #835: a refresh that continues the loaded rows.
describe("applyGraphRefresh", () => {
  const loaded = [row("a", ["main"]), row("b"), row("c"), row("d")];

  it("is the page's rows when the refresh couldn't continue the loaded ones", () => {
    expect(applyGraphRefresh(loaded, page([row("x")], null))).toEqual([row("x")]);
  });

  it("puts new rows and runs of loaded rows in layout order", () => {
    const rows = applyGraphRefresh(
      loaded,
      page([row("n1"), row("n2")], {
        layout: [
          { kind: "New", count: 1 },
          { kind: "Kept", from: 0, count: 2 },
          { kind: "New", count: 1 },
          { kind: "Kept", from: 3, count: 1 },
        ],
        badges: [],
      }),
    );
    expect(rows.map((r) => r.id)).toEqual(["n1", "a", "b", "n2", "d"]);
  });

  it("replaces the badges of kept rows, leaving the loaded rows themselves alone", () => {
    const rows = applyGraphRefresh(
      loaded,
      page([], {
        layout: [{ kind: "Kept", from: 0, count: 4 }],
        badges: [{ id: "a", branchRefs: [], remoteBranchRefs: ["origin/main"], tags: ["v1"] }],
      }),
    );
    expect(rows[0]).toEqual({ ...row("a"), remoteBranchRefs: ["origin/main"], tags: ["v1"] });
    expect(rows.slice(1)).toEqual(loaded.slice(1));
    expect(loaded[0].branchRefs).toEqual(["main"]);
  });
});
