import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import { assignLanes } from "./commitGraphLayout";
import { branchLaneColors, laneColor } from "./laneColors";

function commit(id: string, parentIds: string[], branchRefs: string[] = [], remoteBranchRefs: string[] = []): GraphCommit {
  return {
    id,
    shortId: id,
    summary: id,
    authorName: "a",
    authorEmail: "a@x",
    timestamp: 0,
    parentIds,
    branchRefs,
    remoteBranchRefs,
  };
}

// #673: a swatch shows the colour the graph draws its branch's tip in.
describe("branchLaneColors", () => {
  it("gives each branch the colour of its tip's lane", () => {
    const commits = [
      commit("f", ["b"], ["feature"], ["origin/feature"]),
      commit("m", ["b"], ["main"]),
      commit("b", []),
    ];
    const layouts = assignLanes(commits);
    const colors = branchLaneColors(commits, layouts);

    expect(colors.local.get("feature")).toBe(laneColor(layouts[0].lane));
    expect(colors.local.get("main")).toBe(laneColor(layouts[1].lane));
    expect(colors.local.get("feature")).not.toBe(colors.local.get("main"));
    expect(colors.remote.get("origin/feature")).toBe(colors.local.get("feature"));
  });

  it("has no colour for a branch whose tip isn't loaded", () => {
    const commits = [commit("m", [], ["main"])];
    const colors = branchLaneColors(commits, assignLanes(commits));
    expect(colors.local.has("elsewhere")).toBe(false);
    expect(colors.remote.size).toBe(0);
  });

  it("cycles through the six lane colours", () => {
    expect(laneColor(0)).toBe("var(--color-lane-1)");
    expect(laneColor(6)).toBe("var(--color-lane-1)");
    expect(laneColor(7)).toBe("var(--color-lane-2)");
  });
});
