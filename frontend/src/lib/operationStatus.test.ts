import { describe, expect, it } from "vitest";
import type { RebaseInfo, TerminalRebase, TerminalSequence } from "../ipc/RepoClient";
import {
  abortToast,
  busyReason,
  conflictReason,
  leftoverReason,
  operationBlockers,
  pickToast,
  rebaseAbortSwitchTarget,
  rebaseAbortToast,
  rebaseDivergenceText,
  sequenceLabel,
  sequenceToast,
  stashBlockReason,
  terminalAbortConfirm,
  terminalRebaseLabel,
  terminalRebaseReason,
  terminalRebaseText,
  terminalSequenceLabel,
  terminalSequenceText,
} from "./operationStatus";

describe("operationStatus", () => {
  it("explains what a leftover operation blocks (#240)", () => {
    expect(leftoverReason("CherryPick")).toBe(
      "This cherry-pick's changes are already in the last commit. Commit, amend or abort it first.",
    );
    expect(leftoverReason("Revert")).toBe("This revert's changes are already in the last commit. Commit, amend or abort it first.");
    expect(leftoverReason("Merge")).toBe("This merge's changes are already in the last commit. Commit, amend or abort it first.");
  });

  it("explains blocked steps by conflict count", () => {
    expect(conflictReason(0)).toBeUndefined();
    expect(conflictReason(1)).toBe("Resolve 1 conflict to continue");
    expect(conflictReason(2)).toBe("Resolve 2 conflicts to continue");
  });

  it("labels sequences and builds toasts from outcomes", () => {
    expect(sequenceLabel({ kind: "CherryPick", currentStep: 2, totalSteps: 5, currentCommit: "c" })).toBe(
      "Cherry-picking 2/5",
    );
    expect(sequenceLabel({ kind: "Revert", currentStep: 1, totalSteps: 3, currentCommit: "c" })).toBe("Reverting 1/3");
    expect(pickToast("CherryPick", { kind: "Committed", commitId: "c" }, "1234567890")).toBe("Cherry-picked 1234567");
    expect(pickToast("Revert", { kind: "Committed", commitId: "c" }, "1234567890")).toBe("Reverted 1234567");
    expect(pickToast("CherryPick", { kind: "Conflicted", files: ["a"] }, "1234567890")).toBeNull();
    expect(pickToast("Revert", { kind: "Conflicted", files: ["a"] }, "1234567890")).toBeNull();
    expect(sequenceToast("CherryPick", { kind: "Completed", applied: 3, skipped: [] })).toBe("Cherry-picked 3 commits");
    expect(sequenceToast("Revert", { kind: "Completed", applied: 1, skipped: [] })).toBe("Reverted 1 commit");
    expect(
      sequenceToast("CherryPick", {
        kind: "Stopped",
        kept: "f one",
        progress: { kind: "CherryPick", currentStep: 2, totalSteps: 3, currentCommit: "b" },
      }),
    ).toBe('Stopped after "f one"; its commit was kept. Continue to resume.');
    expect(sequenceToast("CherryPick", { kind: "Completed", applied: 2, skipped: ["x"] })).toBe(
      "Cherry-picked 2 commits, skipped 1 with no changes",
    );
    expect(sequenceToast("Revert", { kind: "Completed", applied: 0, skipped: ["x", "y"] })).toBe(
      "Nothing to revert: 2 commits had no changes",
    );
    expect(sequenceToast("CherryPick", { kind: "Completed", applied: 0, skipped: ["x"] })).toBe(
      "Nothing to cherry-pick: 1 commit had no changes",
    );
    expect(
      sequenceToast("CherryPick", {
        kind: "Conflicted",
        files: ["a"],
        progress: { kind: "CherryPick", currentStep: 2, totalSteps: 3, currentCommit: "c" },
      }),
    ).toBeNull();
    expect(abortToast("CherryPick", { rewound: true })).toBe("Aborted cherry-pick");
    expect(abortToast("Revert", { rewound: true })).toBe("Aborted revert");
    expect(abortToast("CherryPick", { rewound: false })).toBe(
      "Stopped the cherry-pick. HEAD had moved, so it was left where it is.",
    );
    expect(abortToast("Revert", { rewound: false })).toBe(
      "Stopped the revert. HEAD had moved, so it was left where it is.",
    );
  });
});

describe("rebase status text", () => {
  it("rebaseDivergenceText covers each reason", () => {
    expect(rebaseDivergenceText("HeadMoved", "main")).toBe("HEAD was moved outside Browsitory.");
    expect(rebaseDivergenceText("BranchMoved", "main")).toBe("main was changed outside Browsitory.");
    expect(rebaseDivergenceText("BranchMissing", "feature/x")).toBe("feature/x was deleted outside Browsitory.");
    expect(rebaseDivergenceText("OtherOperation", "main")).toBe("Another git operation is in progress.");
  });

  it("rebaseAbortToast for restored, moved, and branch missing", () => {
    const info = (health: RebaseInfo["health"]): RebaseInfo => ({ branch: "topic", onto: "o", currentStep: 1, totalSteps: 2, health });
    expect(rebaseAbortToast({ restored: true }, info({ kind: "Paused" }))).toBe("Aborted rebase");
    expect(rebaseAbortToast({ restored: false }, info({ kind: "Diverged", reason: "HeadMoved" }))).toBe(
      "Stopped the rebase. HEAD had moved, so it was left where it is.",
    );
    expect(rebaseAbortToast({ restored: false }, info({ kind: "Diverged", reason: "BranchMissing" }))).toBe(
      "Stopped the rebase. topic no longer exists, so HEAD was left detached.",
    );
    expect(rebaseAbortToast({ restored: false }, info({ kind: "Diverged", reason: "BranchMoved" }))).toBe(
      "Stopped the rebase. topic had changed, so HEAD was left where it is.",
    );
    expect(rebaseAbortToast({ restored: false }, info({ kind: "Diverged", reason: "OtherOperation" }))).toBe(
      "Stopped the rebase. Another git operation is in progress, so HEAD was left where it is.",
    );
    expect(rebaseAbortSwitchTarget({ restored: false }, info({ kind: "Diverged", reason: "BranchMoved" }))).toBe("topic");
    expect(rebaseAbortSwitchTarget({ restored: true }, info({ kind: "Diverged", reason: "BranchMoved" }))).toBeNull();
    expect(rebaseAbortSwitchTarget({ restored: false }, info({ kind: "Diverged", reason: "HeadMoved" }))).toBeNull();
    expect(rebaseAbortToast({ restored: false }, null)).toBe(
      "Stopped the rebase. HEAD had moved, so it was left where it is.",
    );
  });

  describe("a rebase or git am started in a terminal (#263)", () => {
    const rebase = (overrides: Partial<TerminalRebase> = {}): TerminalRebase => ({
      kind: "Rebase",
      branch: "topic",
      onto: "1a2b3c4d5e6f",
      currentStep: 2,
      totalSteps: 3,
      ...overrides,
    });

    it("terminalRebaseLabel names the branch and the step when known", () => {
      expect(terminalRebaseLabel(rebase())).toBe("Rebasing topic, step 2 of 3 (started in a terminal)");
      expect(terminalRebaseLabel(rebase({ branch: "", currentStep: 0, totalSteps: 0 }))).toBe(
        "Rebasing (started in a terminal)",
      );
      expect(terminalRebaseLabel(rebase({ kind: "Am", branch: "", onto: "", currentStep: 1 }))).toBe(
        "Applying patches, 1 of 3 (started in a terminal)",
      );
    });

    it("terminalRebaseText says to go on in the terminal", () => {
      expect(terminalRebaseText(rebase(), 0)).toBe(
        "Continue or abort it there with git rebase --continue or git rebase --abort.",
      );
      expect(terminalRebaseText(rebase(), 1)).toBe(
        "Resolve 1 conflict, then run git rebase --continue in the terminal.",
      );
      expect(terminalRebaseText(rebase({ kind: "Am" }), 2)).toBe(
        "Resolve 2 conflicts, then run git am --continue in the terminal.",
      );
      expect(terminalRebaseReason(rebase())).toBe("A rebase started in a terminal is in progress.");
      expect(terminalRebaseReason(rebase({ kind: "Am" }))).toBe("A git am started in a terminal is in progress.");
    });
  });

  describe("a sequence started in a terminal (#258, #253)", () => {
    const sequence = (overrides: Partial<TerminalSequence> = {}): TerminalSequence => ({
      kind: "CherryPick",
      pausedOnConflict: false,
      startedAt: "1a2b3c4d5e6f",
      startedAtSummary: "X",
      rewinds: true,
      commitsToRemove: 2,
      ...overrides,
    });

    it("terminalSequenceLabel between steps and paused on a conflict", () => {
      expect(terminalSequenceLabel(sequence())).toBe("Cherry-pick paused in a terminal");
      expect(terminalSequenceLabel(sequence({ kind: "Revert" }))).toBe("Revert paused in a terminal");
      expect(terminalSequenceLabel(sequence({ pausedOnConflict: true }))).toBe("Cherry-picking (started in a terminal)");
      expect(terminalSequenceLabel(sequence({ kind: "Revert", pausedOnConflict: true }))).toBe(
        "Reverting (started in a terminal)",
      );
    });

    it("terminalSequenceText names the command that continues it", () => {
      expect(terminalSequenceText("CherryPick")).toBe(
        "Run git cherry-pick --continue there to apply the remaining commits.",
      );
      expect(terminalSequenceText("Revert")).toBe("Run git revert --continue there to apply the remaining commits.");
    });

    it("terminalAbortConfirm for several commits, one commit and a revert", () => {
      expect(terminalAbortConfirm(sequence())).toEqual({
        title: "Abort the whole cherry-pick?",
        body: 'This also removes the 2 commits it made and puts the branch back at 1a2b3c4 "X". Uncommitted changes stay.',
        confirmLabel: "Abort cherry-pick",
      });
      expect(terminalAbortConfirm(sequence({ commitsToRemove: 1 })).body).toBe(
        'This also removes the commit it made and puts the branch back at 1a2b3c4 "X". Uncommitted changes stay.',
      );
      // A paused step's own changes and resolutions go (#281 review).
      expect(terminalAbortConfirm(sequence({ pausedOnConflict: true })).body).toBe(
        'This also removes the 2 commits it made and puts the branch back at 1a2b3c4 "X". ' +
          "The paused step's changes and conflict resolutions are discarded; other uncommitted changes stay.",
      );
      // The backend stops counting past 1000.
      expect(terminalAbortConfirm(sequence({ commitsToRemove: 1001 })).body).toContain(
        "removes more than 1000 commits it made",
      );
      const revert = terminalAbortConfirm(sequence({ kind: "Revert" }));
      expect(revert.title).toBe("Abort the whole revert?");
      expect(revert.confirmLabel).toBe("Abort revert");
    });
  });
});

describe("operationBlockers (#495)", () => {
  const headInfo = { sequence: null, terminalRebase: null, operation: null, operationLeftover: false };
  const idle = { pending: false, transfer: null, mergeMessage: null, rebaseProgress: null, headInfo };
  const terminalRebase: TerminalRebase = { kind: "Rebase", branch: "topic", onto: "c1", currentStep: 1, totalSteps: 2 };

  it("blocks nothing when nothing runs", () => {
    expect(operationBlockers(idle)).toEqual({ blocked: false, reason: null });
  });

  it.each([
    ["an action is pending", { pending: true }, "Another action is in progress."],
    ["a transfer runs", { transfer: {} }, "A transfer is in progress."],
    ["a sequence is stored", { headInfo: { ...headInfo, sequence: { kind: "Revert" as const, currentStep: 1, totalSteps: 2, currentCommit: "c" } } }, "A revert is in progress."],
    ["a terminal rebase is paused (#503)", { headInfo: { ...headInfo, terminalRebase } }, "A rebase started in a terminal is in progress."],
    ["a terminal git am is paused (#503)", { headInfo: { ...headInfo, terminalRebase: { ...terminalRebase, kind: "Am" as const } } }, "A git am started in a terminal is in progress."],
    ["a merge is in progress", { mergeMessage: "m" }, "A merge is in progress."],
    ["a cherry-pick is in progress", { mergeMessage: "m", headInfo: { ...headInfo, operation: "CherryPick" as const } }, "A cherry-pick is in progress."],
    ["a leftover is there", { mergeMessage: "m", headInfo: { ...headInfo, operation: "Merge" as const, operationLeftover: true } }, leftoverReason("Merge")],
    ["a rebase is in progress", { rebaseProgress: { currentStep: 1, totalSteps: 2 } }, "A rebase is in progress."],
  ])("blocks while %s, saying so", (_label, overrides, reason) => {
    expect(operationBlockers({ ...idle, ...overrides })).toEqual({ blocked: true, reason });
  });

  it("names a terminal rebase before the MERGE_MSG its conflicted step writes", () => {
    expect(operationBlockers({ ...idle, mergeMessage: "m", headInfo: { ...headInfo, terminalRebase } }).reason).toBe(
      terminalRebaseReason(terminalRebase),
    );
  });
});

describe("busyReason (#521)", () => {
  it("blocks only while an action is pending or a transfer runs", () => {
    expect(busyReason({ pending: false, transfer: null })).toBeNull();
    expect(busyReason({ pending: true, transfer: null })).toBe("Another action is in progress.");
    expect(busyReason({ pending: false, transfer: {} })).toBe("A transfer is in progress.");
  });
});

describe("stashBlockReason (#495)", () => {
  const headInfo = { sequence: null, terminalRebase: null };
  const changes = [{}];

  it("allows stashing changes, even during a merge", () => {
    expect(stashBlockReason({ status: changes, rebaseProgress: null, headInfo })).toBeNull();
  });

  it("refuses without changes and during a paused rebase, sequence or terminal rebase", () => {
    expect(stashBlockReason({ status: [], rebaseProgress: null, headInfo })).toBe("No changes to stash.");
    expect(stashBlockReason({ status: changes, rebaseProgress: { currentStep: 1, totalSteps: 2 }, headInfo })).toBe(
      "A rebase is in progress.",
    );
    expect(
      stashBlockReason({
        status: changes,
        rebaseProgress: null,
        headInfo: { ...headInfo, sequence: { kind: "CherryPick", currentStep: 1, totalSteps: 2, currentCommit: "c" } },
      }),
    ).toBe("A cherry-pick is in progress.");
    expect(
      stashBlockReason({
        status: changes,
        rebaseProgress: null,
        headInfo: { ...headInfo, terminalRebase: { kind: "Am", branch: "", onto: "", currentStep: 0, totalSteps: 0 } },
      }),
    ).toBe("A git am started in a terminal is in progress.");
  });
});
