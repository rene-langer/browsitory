import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../../ipc/RepoClient";
import { commitActions, commitDropActions } from "./commitActions";

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
    expect(actions.checkout).toMatchObject({ label: "Switch to (detached)", paletteLabel: "Switch to abc1234 (detached)" });
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

// #744: commits dragged from the graph onto a branch in the sidebar. A cherry-pick lands on the
// current branch, so only it takes the drop; the rules are the commit menu's Cherry-pick and the
// multi-selection's "Cherry-pick N commits".
describe("commitDropActions", () => {
  const branches = [
    { name: "main", isCurrent: true },
    { name: "feature", isCurrent: false },
  ];
  const onMain = { kind: "local" as const, name: "main" };
  const plain = commit("aaa1111aa", ["p"]);
  const other = commit("bbb2222bb", ["aaa1111aa"]);
  const merge = commit("ccc3333cc", ["p1", "p2"]);

  it("offers cherry-picking one commit dropped on the current branch", () => {
    expect(commitDropActions(free, branches, { commits: [plain], headId: "head" }, onMain)).toEqual({
      kind: "menu",
      item: {
        label: "Cherry-pick aaa1111 onto main",
        paletteLabel: "Cherry-pick aaa1111 onto main",
        enabled: true,
        reason: undefined,
        run: { kind: "pick", commitId: "aaa1111aa" },
      },
    });
  });

  it("sends a merge commit to the parent picker", () => {
    const drop = commitDropActions(free, branches, { commits: [merge], headId: "head" }, onMain);
    expect(drop.kind === "menu" && [drop.item.label, drop.item.run]).toEqual([
      "Cherry-pick ccc3333 onto main…",
      { kind: "choose-parent", commitId: "ccc3333cc" },
    ]);
  });

  it("offers cherry-picking several commits in the order given (oldest first)", () => {
    const drop = commitDropActions(free, branches, { commits: [plain, other], headId: "head" }, onMain);
    expect(drop.kind === "menu" && [drop.item.label, drop.item.run]).toEqual([
      "Cherry-pick 2 commits onto main",
      { kind: "pick-many", commitIds: ["aaa1111aa", "bbb2222bb"] },
    ]);
  });

  it("refuses several commits with a merge among them, as the multi-selection's item does", () => {
    expect(commitDropActions(free, branches, { commits: [plain, merge], headId: "head" }, onMain)).toEqual({
      kind: "refused",
      reason: "Cherry-pick merge commits one at a time.",
    });
  });

  it("refuses a drop on a branch that isn't current, naming the current one", () => {
    const expected = {
      kind: "refused",
      reason: "Only the current branch, main, can take a cherry-pick: drop on main.",
    };
    expect(commitDropActions(free, branches, { commits: [plain], headId: "head" }, { kind: "local", name: "feature" })).toEqual(expected);
    expect(
      commitDropActions(free, branches, { commits: [plain], headId: "head" }, { kind: "remote", remoteName: "origin", branchName: "main" }),
    ).toEqual(expected);
  });

  it("refuses every drop on a detached HEAD", () => {
    const detached = [{ name: "feature", isCurrent: false }];
    expect(commitDropActions(free, detached, { commits: [plain], headId: "head" }, { kind: "local", name: "feature" })).toEqual({
      kind: "refused",
      reason: "Check out a branch first: a cherry-pick applies to the current branch.",
    });
  });

  it("refuses with the busy reason while another operation runs, for one commit or several", () => {
    expect(commitDropActions(busy, branches, { commits: [plain], headId: "head" }, onMain)).toEqual({
      kind: "refused",
      reason: busy.reason,
    });
    expect(commitDropActions(busy, branches, { commits: [plain, other], headId: "head" }, onMain)).toEqual({
      kind: "refused",
      reason: busy.reason,
    });
  });

  it("refuses HEAD itself, as the menu's Cherry-pick does", () => {
    expect(commitDropActions(free, branches, { commits: [plain], headId: "aaa1111aa" }, onMain)).toEqual({
      kind: "refused",
      reason: "HEAD already contains this commit.",
    });
  });
});
