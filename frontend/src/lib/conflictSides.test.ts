import { describe, expect, it } from "vitest";
import type { BranchInfo, HeadInfo } from "../ipc/RepoClient";
import { conflictSideNames } from "./conflictSides";

const idle: HeadInfo = {
  headId: "abc1234567890",
  detachedAt: null,
  headPushed: false,
  operation: null,
  operationLeftover: false,
  squashMessage: null,
  commitHooks: [],
  sequence: null,
  rebase: null,
  bisect: null,
  terminalSequence: null,
  terminalRebase: null,
};
const branches: BranchInfo[] = [
  { name: "feature", isCurrent: false },
  { name: "main", isCurrent: true },
];

describe("conflictSideNames", () => {
  it("names the current branch and the merged branch for a merge", () => {
    const head = { ...idle, operation: "Merge" as const };
    expect(conflictSideNames(head, branches, "Merge branch 'feature'\n\n# Conflicts:\n")).toEqual({
      ours: "main",
      theirs: "feature",
    });
  });

  it("reads a remote-tracking branch and a merged commit from the merge message", () => {
    const head = { ...idle, operation: "Merge" as const };
    expect(conflictSideNames(head, branches, "Merge remote-tracking branch 'origin/main'\n").theirs).toBe(
      "origin/main",
    );
    expect(conflictSideNames(head, branches, "Merge branch 'topic' of https://example.com/x\n").theirs).toBe("topic");
    expect(conflictSideNames(head, branches, "Merge commit 'abc1234'\n").theirs).toBe("abc1234");
  });

  it("falls back to a description when the merge message names nothing", () => {
    const head = { ...idle, operation: "Merge" as const };
    expect(conflictSideNames(head, branches, null)).toEqual({ ours: "main", theirs: "incoming" });
  });

  it("names the base and the rebased branch during a paused rebase", () => {
    const head: HeadInfo = {
      ...idle,
      rebase: { branch: "feature", onto: "0123456789abcdef", currentStep: 1, totalSteps: 2, health: { kind: "Paused" } },
    };
    expect(conflictSideNames(head, branches, null)).toEqual({ ours: "onto 0123456", theirs: "feature" });
  });

  it("names a terminal rebase the same way, when it knows the branch", () => {
    const head: HeadInfo = {
      ...idle,
      terminalRebase: { kind: "Rebase", branch: "feature", onto: "fedcba9876543210", currentStep: 1, totalSteps: 1 },
    };
    expect(conflictSideNames(head, [], null)).toEqual({ ours: "onto fedcba9", theirs: "feature" });
  });

  it("names a cherry-pick's and a revert's incoming side", () => {
    expect(conflictSideNames({ ...idle, operation: "CherryPick" }, branches, "pick me\n")).toEqual({
      ours: "main",
      theirs: "picked commit",
    });
    expect(conflictSideNames({ ...idle, operation: "Revert" }, branches, null).theirs).toBe("revert");
  });

  it("uses the detached commit when no branch is current", () => {
    const head = { ...idle, operation: "Merge" as const, detachedAt: "abc1234567890" };
    expect(conflictSideNames(head, [], "Merge branch 'feature'").ours).toBe("abc1234");
  });

  it("names only the current branch when no operation is known (a stash applied with conflicts)", () => {
    expect(conflictSideNames(idle, branches, null)).toEqual({ ours: "main", theirs: null });
    expect(conflictSideNames({ ...idle, headId: null }, [], null)).toEqual({ ours: null, theirs: null });
  });
});
