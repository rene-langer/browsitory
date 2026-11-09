import { describe, expect, it } from "vitest";
import { branchActions, remoteBranchActions } from "./branchActions";

const free = { blocked: false, reason: null };
const busy = { blocked: true, reason: "A merge is in progress." };
const upstream = { localBranch: "main", remoteName: "origin", remoteBranch: "main" };
const main = { name: "main", isCurrent: true };
const feature = { name: "feature", isCurrent: false };

describe("branchActions", () => {
  it("offers switching to and merging another branch, under palette labels naming it", () => {
    const actions = branchActions({ blockers: free, remoteCount: 1, upstream }, feature);
    expect(actions.checkout).toEqual({ label: "Checkout", paletteLabel: "Switch to feature", enabled: true, reason: undefined });
    expect(actions.merge).toMatchObject({ paletteLabel: "Merge feature into current branch…", enabled: true });
    expect(actions.pushToUpstream).toBeNull();
    expect(actions.setUpstream).toBeNull();
  });

  it("refuses to switch to or merge the current branch, saying why", () => {
    const actions = branchActions({ blockers: free, remoteCount: 1, upstream }, main);
    expect(actions.checkout).toMatchObject({ enabled: false, reason: "Already the current branch." });
    expect(actions.merge.enabled).toBe(false);
    expect(actions.pushToUpstream).toMatchObject({ label: "Push to origin", enabled: true });
    expect(actions.setUpstream).toMatchObject({ label: "Set upstream…", enabled: true });
  });

  it("disables every action with the busy reason, which wins over a refusal", () => {
    const actions = branchActions({ blockers: busy, remoteCount: 0, upstream }, main);
    for (const action of [actions.checkout, actions.rename, actions.merge, actions.pushToUpstream!, actions.setUpstream!, actions.pushTo, actions.delete]) {
      expect(action).toMatchObject({ enabled: false, reason: busy.reason });
    }
  });

  it("disables Push to… without a remote", () => {
    expect(branchActions({ blockers: free, remoteCount: 0, upstream: null }, feature).pushTo).toMatchObject({
      enabled: false,
      reason: "No remote configured.",
    });
  });
});

describe("remoteBranchActions", () => {
  it("names the branch in Delete and follows the busy rule", () => {
    expect(remoteBranchActions(free, "origin", "topic").delete).toMatchObject({ label: "Delete origin/topic…", enabled: true });
    for (const action of Object.values(remoteBranchActions(busy, "origin", "topic"))) {
      expect(action).toMatchObject({ enabled: false, reason: busy.reason });
    }
  });
});
