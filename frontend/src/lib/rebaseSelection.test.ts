import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import { planRebaseSelection, presetForAction } from "./rebaseSelection";

function commit(id: string, parentIds: string[]): GraphCommit {
  return {
    id,
    shortId: id,
    summary: id,
    authorName: "Rene",
    authorEmail: "rene@example.com",
    timestamp: 1,
    parentIds,
    branchRefs: [],
    remoteBranchRefs: [],
  };
}

// Newest-first chain: E -> D -> C -> B -> A (A is a root, so it has no parent).
const chain = [
  commit("E", ["D"]),
  commit("D", ["C"]),
  commit("C", ["B"]),
  commit("B", ["A"]),
  commit("A", []),
];

describe("planRebaseSelection", () => {
  it("needs at least two selected commits", () => {
    const result = planRebaseSelection(chain, new Set(["C"]));
    expect(result).toEqual({ ok: false, reason: "Select two or more commits." });
  });

  it("bases on the parent of the oldest selected commit, oldest first", () => {
    const result = planRebaseSelection(chain, new Set(["D", "B"]));
    expect(result).toEqual({
      ok: true,
      selection: { onto: "A", contiguous: false, idsOldestFirst: ["B", "D"] },
    });
  });

  it("marks an unbroken first-parent run as contiguous", () => {
    const result = planRebaseSelection(chain, new Set(["D", "C", "B"]));
    expect(result).toMatchObject({ ok: true, selection: { onto: "A", contiguous: true } });
  });

  it("is not contiguous across a fork point even when graph rows are adjacent", () => {
    const fork = [commit("F1", ["M1"]), commit("M2", ["M1"]), commit("M1", ["R"])];
    const result = planRebaseSelection(fork, new Set(["F1", "M2"]));
    expect(result).toMatchObject({ ok: true, selection: { contiguous: false } });
  });

  it("rejects a selection containing a merge commit", () => {
    const withMerge = [commit("M", ["B", "X"]), commit("B", ["A"]), commit("A", ["R"])];
    const result = planRebaseSelection(withMerge, new Set(["M", "B"]));
    expect(result).toEqual({
      ok: false,
      reason: "Merge and root commits can't be rebased.",
    });
  });

  it("rejects a selection whose oldest commit is a root commit", () => {
    const result = planRebaseSelection(chain, new Set(["B", "A"]));
    expect(result).toEqual({
      ok: false,
      reason: "Merge and root commits can't be rebased.",
    });
  });

  it("ignores selected ids that are not in the loaded commits", () => {
    const result = planRebaseSelection(chain, new Set(["D", "B", "gone"]));
    expect(result).toMatchObject({ ok: true, selection: { idsOldestFirst: ["B", "D"] } });
  });
});

describe("presetForAction", () => {
  const selection = { onto: "A", contiguous: true, idsOldestFirst: ["B", "C", "D"] };

  it("Squash and Fixup skip the oldest commit, which has nothing before it to fold into", () => {
    expect(presetForAction(selection, "Squash")).toEqual(
      new Map([
        ["C", "Squash"],
        ["D", "Squash"],
      ]),
    );
    expect(presetForAction(selection, "Fixup")).toEqual(
      new Map([
        ["C", "Fixup"],
        ["D", "Fixup"],
      ]),
    );
  });

  it("Drop applies to every selected commit", () => {
    expect(presetForAction(selection, "Drop")).toEqual(
      new Map([
        ["B", "Drop"],
        ["C", "Drop"],
        ["D", "Drop"],
      ]),
    );
  });
});
