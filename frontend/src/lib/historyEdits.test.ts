import { describe, expect, it } from "vitest";
import type { GraphCommit, HeadInfo, RebasePlanCommit, StatusEntry } from "../ipc/RepoClient";
import { buildHistoryEditPlan, historyEditChecks, type HistoryEditContext } from "./historyEdits";

function commit(id: string, parentIds: string[]): GraphCommit {
  return {
    id,
    shortId: id,
    summary: `${id} summary`,
    authorName: "Rene",
    authorEmail: "rene@example.com",
    timestamp: 1,
    parentIds,
    branchRefs: [],
    remoteBranchRefs: [],
  };
}

const HEAD: HeadInfo = {
  headId: "E",
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

// Newest-first: E -> D -> C -> B -> A (root). X is on another branch, off C.
const chain = [
  commit("E", ["D"]),
  commit("X", ["C"]),
  commit("D", ["C"]),
  commit("C", ["B"]),
  commit("B", ["A"]),
  commit("A", []),
];

function context(overrides: Partial<HistoryEditContext> = {}): HistoryEditContext {
  return { commits: chain, headInfo: HEAD, currentBranch: "main", status: [], ...overrides };
}

describe("historyEditChecks", () => {
  it("bases each edit of a middle commit on the right commit", () => {
    expect(historyEditChecks(context(), "C")).toEqual({
      Reword: { ok: true, onto: "B" },
      Squash: { ok: true, onto: "A" },
      Fixup: { ok: true, onto: "A" },
      MoveUp: { ok: true, onto: "B" },
      MoveDown: { ok: true, onto: "A" },
      Drop: { ok: true, onto: "B" },
      Edit: { ok: true, onto: "B" },
    });
  });

  it("can't move HEAD up", () => {
    const checks = historyEditChecks(context(), "E");
    expect(checks.MoveUp).toEqual({ ok: false, reason: "This is already the newest commit." });
    expect(checks.MoveDown).toEqual({ ok: true, onto: "C" });
    expect(checks.Drop).toEqual({ ok: true, onto: "D" });
  });

  it("refuses to rewrite the root commit or to fold into it", () => {
    const root = historyEditChecks(context(), "A");
    const rootReason = "The first commit of the history can't be edited here.";
    for (const check of Object.values(root)) expect(check).toEqual({ ok: false, reason: rootReason });
    const child = historyEditChecks(context(), "B");
    const parentRoot = "Its parent is the first commit of the history, which can't be edited here.";
    expect(child.Squash).toEqual({ ok: false, reason: parentRoot });
    expect(child.Fixup).toEqual({ ok: false, reason: parentRoot });
    expect(child.MoveDown).toEqual({ ok: false, reason: parentRoot });
    expect(child.Reword).toEqual({ ok: true, onto: "A" });
    expect(child.MoveUp).toEqual({ ok: true, onto: "A" });
    expect(child.Drop).toEqual({ ok: true, onto: "A" });
  });

  it("disables everything for a commit that isn't on the current branch", () => {
    for (const check of Object.values(historyEditChecks(context(), "X"))) {
      expect(check).toEqual({ ok: false, reason: "Only commits of the current branch can be edited." });
    }
  });

  it("disables everything for a merge commit, and below one", () => {
    const commits = [commit("M", ["E", "X"]), ...chain];
    const headInfo = { ...HEAD, headId: "M" };
    for (const check of Object.values(historyEditChecks(context({ commits, headInfo }), "M"))) {
      expect(check).toEqual({ ok: false, reason: "Merge commits can't be edited here." });
    }
    for (const check of Object.values(historyEditChecks(context({ commits, headInfo }), "C"))) {
      expect(check).toEqual({
        ok: false,
        reason: "A merge commit comes after this one. Use Rebase onto here… instead.",
      });
    }
  });

  it("refuses to squash into or move below a merge parent", () => {
    const commits = [commit("T", ["M"]), commit("M", ["E", "X"]), ...chain];
    const checks = historyEditChecks(context({ commits, headInfo: { ...HEAD, headId: "T" } }), "T");
    const reason = "Its parent is a merge commit, which can't be edited here.";
    expect(checks.Squash).toEqual({ ok: false, reason });
    expect(checks.MoveDown).toEqual({ ok: false, reason });
    expect(checks.Drop).toEqual({ ok: true, onto: "M" });
  });

  it("needs a clean tracked tree but ignores untracked files", () => {
    const untracked: StatusEntry = { path: "new.txt", oldPath: null, staged: false, kind: "New" };
    expect(historyEditChecks(context({ status: [untracked] }), "C").Drop).toEqual({ ok: true, onto: "B" });
    const modified: StatusEntry = { path: "a.txt", oldPath: null, staged: false, kind: "Modified" };
    const staged: StatusEntry = { path: "b.txt", oldPath: null, staged: true, kind: "New" };
    for (const status of [[modified], [staged]]) {
      for (const check of Object.values(historyEditChecks(context({ status }), "C"))) {
        expect(check).toEqual({ ok: false, reason: "Commit or stash your changes first." });
      }
    }
  });

  it("needs a branch and no operation in progress", () => {
    const detached = historyEditChecks(context({ currentBranch: null, headInfo: { ...HEAD, detachedAt: "E" } }), "C");
    expect(detached.Drop).toEqual({ ok: false, reason: "Check out a branch first." });
    const busy = "Finish or abort the operation in progress first.";
    const paused: Partial<HeadInfo>[] = [
      { operation: "Merge" },
      { rebase: { currentStep: 1, totalSteps: 2, onto: "B", branch: "main", health: { kind: "Paused" } } },
      { sequence: { kind: "CherryPick", currentStep: 1, totalSteps: 2, currentCommit: "X" } },
      { terminalRebase: { kind: "Rebase", branch: "main", onto: "B", currentStep: 1, totalSteps: 2 } },
      {
        terminalSequence: {
          kind: "CherryPick",
          pausedOnConflict: false,
          startedAt: "E",
          startedAtSummary: "E",
          rewinds: true,
          commitsToRemove: 1,
        },
      },
    ];
    for (const extra of paused) {
      expect(historyEditChecks(context({ headInfo: { ...HEAD, ...extra } }), "C").Drop).toEqual({ ok: false, reason: busy });
    }
  });

  it("treats a commit beyond the loaded history as not on the branch", () => {
    expect(historyEditChecks(context(), "unknown").Drop).toEqual({
      ok: false,
      reason: "Only commits of the current branch can be edited.",
    });
  });
});

function planCommit(id: string): RebasePlanCommit {
  return { id, shortId: id, summary: id, authorName: "Rene", timestamp: 1 };
}

const pick = (commitId: string) => ({ commitId, action: { kind: "Pick" as const }, combinedMessage: null });

describe("buildHistoryEditPlan", () => {
  // commitsSince(onto) answers oldest-first.
  const fromB = ["C", "D", "E"].map(planCommit);
  const fromA = ["B", "C", "D", "E"].map(planCommit);

  it("rewords one commit and picks the rest", () => {
    expect(buildHistoryEditPlan({ kind: "Reword", message: "new\n" }, "C", fromB)).toEqual([
      { commitId: "C", action: { kind: "Reword", message: "new\n" }, combinedMessage: null },
      pick("D"),
      pick("E"),
    ]);
  });

  it("squashes into the parent with the combined message, or fixes up keeping the parent's", () => {
    expect(buildHistoryEditPlan({ kind: "Squash", combinedMessage: "both" }, "C", fromA)).toEqual([
      { commitId: "B", action: { kind: "Pick" }, combinedMessage: "both" },
      { commitId: "C", action: { kind: "Squash" }, combinedMessage: null },
      pick("D"),
      pick("E"),
    ]);
    expect(buildHistoryEditPlan({ kind: "Fixup" }, "C", fromA)).toEqual([
      pick("B"),
      { commitId: "C", action: { kind: "Fixup" }, combinedMessage: null },
      pick("D"),
      pick("E"),
    ]);
  });

  it("moves a commit up or down by swapping it with its neighbour", () => {
    expect(buildHistoryEditPlan({ kind: "MoveUp" }, "C", fromB)).toEqual([pick("D"), pick("C"), pick("E")]);
    expect(buildHistoryEditPlan({ kind: "MoveDown" }, "C", fromA)).toEqual([pick("C"), pick("B"), pick("D"), pick("E")]);
  });

  it("drops one commit", () => {
    expect(buildHistoryEditPlan({ kind: "Drop" }, "C", fromB)).toEqual([
      { commitId: "C", action: { kind: "Drop" }, combinedMessage: null },
      pick("D"),
      pick("E"),
    ]);
  });

  it("refuses when the history no longer has the commit where the edit expects it", () => {
    const changed = "The history changed. Refresh and try again.";
    expect(() => buildHistoryEditPlan({ kind: "Drop" }, "Z", fromB)).toThrow(changed);
    expect(() => buildHistoryEditPlan({ kind: "Drop" }, "C", fromA)).toThrow(changed);
    expect(() => buildHistoryEditPlan({ kind: "Fixup" }, "C", fromB)).toThrow(changed);
    expect(() => buildHistoryEditPlan({ kind: "MoveUp" }, "E", ["E"].map(planCommit))).toThrow(changed);
  });
});
