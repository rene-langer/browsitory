import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../../ipc/RepoClient";
import { commitActions } from "./commitActions";

const commit = (id: string, parentIds: string[]): GraphCommit => ({
  id,
  shortId: id.slice(0, 7),
  summary: id,
  authorName: "A",
  authorEmail: "a@x",
  timestamp: 0,
  parentIds,
  branchRefs: [],
  remoteBranchRefs: [],
});
const free = { blocked: false, reason: null };
const busy = { blocked: true, reason: "A rebase started in a terminal is in progress." };

describe("commitActions", () => {
  it("enables every action on an ordinary commit while nothing blocks", () => {
    const actions = commitActions({ blockers: free, headId: "head000", currentBranch: "main" }, commit("abc1234", ["p"]));
    for (const action of Object.values(actions)) {
      expect(action.enabled).toBe(true);
      expect(action.reason).toBeUndefined();
    }
    expect(actions.cherryPick).toMatchObject({ label: "Cherry-pick", paletteLabel: "Cherry-pick abc1234", needsParent: false });
    expect(actions.revert).toMatchObject({ label: "Revert", paletteLabel: "Revert abc1234" });
    expect(actions.checkout).toMatchObject({ label: "Checkout (detached)", paletteLabel: "Checkout abc1234 (detached)" });
    expect(actions.reset).toMatchObject({ label: "Reset main to here…", paletteLabel: "Reset to abc1234…" });
    expect(actions.createTag).toMatchObject({ label: "Create tag…", paletteLabel: "Create tag at abc1234…" });
  });

  it("disables the mutating actions with the blocking reason, but not Create tag (its dialog guards)", () => {
    const actions = commitActions({ blockers: busy, headId: "head000", currentBranch: "main" }, commit("abc1234", ["p"]));
    for (const action of [actions.cherryPick, actions.revert, actions.checkout, actions.reset]) {
      expect(action.enabled).toBe(false);
      expect(action.reason).toBe(busy.reason);
    }
    expect(actions.createTag.enabled).toBe(true);
  });

  it("refuses to cherry-pick HEAD and to reset without a current branch, saying why", () => {
    const actions = commitActions({ blockers: free, headId: "abc1234", currentBranch: null }, commit("abc1234", ["p"]));
    expect(actions.cherryPick).toMatchObject({ enabled: false, reason: "HEAD already contains this commit." });
    expect(actions.reset).toMatchObject({ enabled: false, reason: "Check out a branch first.", label: "Reset HEAD to here…" });
    expect(actions.revert.enabled).toBe(true);
    expect(actions.checkout.enabled).toBe(true);
  });

  it("asks for a parent on a merge commit", () => {
    const actions = commitActions({ blockers: free, headId: null, currentBranch: "main" }, commit("abc1234", ["p1", "p2"]));
    expect(actions.cherryPick).toMatchObject({ label: "Cherry-pick…", paletteLabel: "Cherry-pick abc1234…", needsParent: true });
    expect(actions.revert).toMatchObject({ label: "Revert…", paletteLabel: "Revert abc1234…", needsParent: true });
  });
});
