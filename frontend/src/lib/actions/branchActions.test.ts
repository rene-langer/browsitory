import { describe, expect, it } from "vitest";
import { branchActions, branchDropActions, rebaseOntoAction, remoteBranchActions } from "./branchActions";

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

// #619: "Rebase current branch onto <name>…" in the branch menus and the palette.
describe("rebaseOntoAction", () => {
  it("rebases onto another local branch by its full ref name", () => {
    expect(rebaseOntoAction(free, "main", { kind: "local", name: "feature" })).toEqual({
      label: "Rebase current branch onto feature…",
      paletteLabel: "Rebase current branch onto feature…",
      onto: "refs/heads/feature",
      enabled: true,
      reason: undefined,
    });
  });

  it("rebases onto a remote-tracking branch by its full ref name", () => {
    expect(rebaseOntoAction(free, "main", { kind: "remote", remoteName: "origin", branchName: "feat/x" })).toMatchObject({
      label: "Rebase current branch onto origin/feat/x…",
      onto: "refs/remotes/origin/feat/x",
      enabled: true,
    });
  });

  it("refuses the current branch, saying why", () => {
    expect(rebaseOntoAction(free, "main", { kind: "local", name: "main" })).toMatchObject({
      enabled: false,
      reason: "Already the current branch.",
    });
  });

  it("is absent without a current branch (detached HEAD)", () => {
    expect(rebaseOntoAction(free, null, { kind: "local", name: "feature" })).toBeNull();
    expect(rebaseOntoAction(free, null, { kind: "remote", remoteName: "origin", branchName: "main" })).toBeNull();
  });

  it("is disabled with the busy reason during a merge, rebase or pick", () => {
    expect(rebaseOntoAction(busy, "main", { kind: "local", name: "feature" })).toMatchObject({
      enabled: false,
      reason: busy.reason,
    });
    expect(rebaseOntoAction(busy, "main", { kind: "local", name: "main" })).toMatchObject({
      enabled: false,
      reason: busy.reason,
    });
    expect(rebaseOntoAction(busy, "main", { kind: "remote", remoteName: "origin", branchName: "main" })).toMatchObject({
      enabled: false,
      reason: busy.reason,
    });
  });
});

// #744: a branch dropped on a branch. Only the current branch can take a merge or a rebase, so
// a drop must involve it; the items are the menus' Merge and Rebase under labels naming both.
describe("branchDropActions", () => {
  const other = { name: "other", isCurrent: false };
  const branches = [main, feature, other];
  const local = (name: string) => ({ kind: "local" as const, name });
  const remote = (remoteName: string, branchName: string) => ({ kind: "remote" as const, remoteName, branchName });

  it("offers merging a branch dropped on the current branch first, then rebasing onto it", () => {
    const drop = branchDropActions(free, branches, local("feature"), local("main"));
    expect(drop).toEqual({
      kind: "menu",
      items: [
        { label: "Merge feature into main…", paletteLabel: "Merge feature into main…", enabled: true, reason: undefined, run: { kind: "merge", name: "feature" } },
        { label: "Rebase main onto feature…", paletteLabel: "Rebase main onto feature…", enabled: true, reason: undefined, run: { kind: "rebase", onto: "refs/heads/feature" } },
      ],
    });
  });

  it("offers rebasing the current branch dropped on a branch first, then merging that branch", () => {
    const drop = branchDropActions(free, branches, local("main"), local("feature"));
    expect(drop.kind === "menu" && drop.items.map((item) => [item.label, item.run])).toEqual([
      ["Rebase main onto feature…", { kind: "rebase", onto: "refs/heads/feature" }],
      ["Merge feature into main…", { kind: "merge", name: "feature" }],
    ]);
  });

  it("offers only rebasing when a remote branch is involved", () => {
    const onto = { kind: "rebase", onto: "refs/remotes/origin/feature" };
    const fromRemote = branchDropActions(free, branches, remote("origin", "feature"), local("main"));
    expect(fromRemote.kind === "menu" && fromRemote.items.map((item) => [item.label, item.run])).toEqual([
      ["Rebase main onto origin/feature…", onto],
    ]);
    const ontoRemote = branchDropActions(free, branches, local("main"), remote("origin", "feature"));
    expect(ontoRemote.kind === "menu" && ontoRemote.items.map((item) => [item.label, item.run])).toEqual([
      ["Rebase main onto origin/feature…", onto],
    ]);
  });

  it("refuses a drop between two branches that aren't current, naming the current one", () => {
    expect(branchDropActions(free, branches, local("feature"), local("other"))).toEqual({
      kind: "refused",
      reason: "Only the current branch, main, can be merged into or rebased: drop on main, or drag main.",
    });
    expect(branchDropActions(free, branches, remote("origin", "x"), remote("origin", "y")).kind).toBe("refused");
  });

  it("refuses every drop on a detached HEAD", () => {
    const detached = [feature, other];
    expect(branchDropActions(free, detached, local("feature"), local("other"))).toEqual({
      kind: "refused",
      reason: "Check out a branch first: a drop merges into or rebases the current branch.",
    });
  });

  it("refuses with the busy reason while another operation runs", () => {
    expect(branchDropActions(busy, branches, local("feature"), local("main"))).toEqual({
      kind: "refused",
      reason: busy.reason,
    });
  });

  it("is a no-op for a branch dropped on itself", () => {
    expect(branchDropActions(free, branches, local("main"), local("main"))).toEqual({ kind: "none" });
    expect(branchDropActions(free, branches, remote("origin", "x"), remote("origin", "x"))).toEqual({ kind: "none" });
  });
});
