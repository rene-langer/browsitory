import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import { useCommitGraphLayout } from "./useCommitGraphLayout";

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

describe("useCommitGraphLayout", () => {
  // #836: the graph went blank once about 30,000 rows were loaded, because the lane count
  // spread every row's lanes into one `Math.max` call, past the engine's argument limit.
  it("lays out 200,000 loaded rows and counts their lanes", () => {
    const count = 200_000;
    const commits = Array.from({ length: count }, (_, i) => {
      const n = count - 1 - i;
      return commit(`c${n}`, n === 0 ? [] : [`c${n - 1}`]);
    });

    const { result } = renderHook(() => useCommitGraphLayout(commits, {}));

    expect(result.current.layouts).toHaveLength(count);
    expect(result.current.laneCount).toBe(1);
  });
});
