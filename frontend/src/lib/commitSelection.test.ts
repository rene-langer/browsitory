import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import {
  EMPTY_SELECTION,
  extendSelection,
  selectOnly,
  toggleCommit,
} from "./commitSelection";

function commit(id: string): GraphCommit {
  return {
    id,
    shortId: id,
    summary: id,
    authorName: "Rene",
    authorEmail: "rene@example.com",
    timestamp: 1,
    parentIds: [],
    branchRefs: [],
    remoteBranchRefs: [],
  };
}

// Newest-first, like the graph: E is the newest, A the oldest.
const commits = ["E", "D", "C", "B", "A"].map(commit);

describe("commitSelection", () => {
  it("selectOnly selects one commit and anchors on it", () => {
    expect(selectOnly("C")).toEqual({ ids: new Set(["C"]), anchorId: "C" });
  });

  it("toggleCommit adds an unselected commit and moves the anchor to it", () => {
    const next = toggleCommit(selectOnly("E"), "C");
    expect(next.ids).toEqual(new Set(["E", "C"]));
    expect(next.anchorId).toBe("C");
  });

  it("toggleCommit removes a selected commit and re-anchors on a remaining one", () => {
    const both = toggleCommit(selectOnly("E"), "C");
    const next = toggleCommit(both, "C");
    expect(next.ids).toEqual(new Set(["E"]));
    expect(next.anchorId).toBe("E");
  });

  it("toggleCommit removing the last commit yields an empty selection", () => {
    expect(toggleCommit(selectOnly("E"), "E")).toEqual(EMPTY_SELECTION);
  });

  it("extendSelection selects the whole range between anchor and target, in either direction", () => {
    expect(extendSelection(selectOnly("D"), commits, "B").ids).toEqual(new Set(["D", "C", "B"]));
    expect(extendSelection(selectOnly("B"), commits, "D").ids).toEqual(new Set(["D", "C", "B"]));
  });

  it("extendSelection keeps the anchor so a later shift-click pivots on it", () => {
    const first = extendSelection(selectOnly("D"), commits, "B");
    expect(first.anchorId).toBe("D");
    expect(extendSelection(first, commits, "E").ids).toEqual(new Set(["E", "D"]));
  });

  it("extendSelection without a usable anchor falls back to selecting just the target", () => {
    expect(extendSelection(EMPTY_SELECTION, commits, "C")).toEqual(selectOnly("C"));
    expect(extendSelection({ ids: new Set(["Z"]), anchorId: "Z" }, commits, "C")).toEqual(
      selectOnly("C"),
    );
  });
});
