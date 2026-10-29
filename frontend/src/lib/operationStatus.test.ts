import { describe, expect, it } from "vitest";
import type { RebaseInfo } from "../ipc/RepoClient";
import {
  abortToast,
  conflictReason,
  pickToast,
  rebaseAbortToast,
  rebaseDivergenceText,
  sequenceLabel,
  sequenceToast,
} from "./operationStatus";

describe("operationStatus", () => {
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
    expect(pickToast("CherryPick", { kind: "Committed", commitId: "c" })).toBe("Cherry-picked");
    expect(pickToast("Revert", { kind: "Committed", commitId: "c" })).toBe("Reverted");
    expect(pickToast("CherryPick", { kind: "Conflicted", files: ["a"] })).toBeNull();
    expect(pickToast("Revert", { kind: "Conflicted", files: ["a"] })).toBeNull();
    expect(sequenceToast("CherryPick", { kind: "Completed", applied: 3, skipped: [] })).toBe("Cherry-picked 3 commits");
    expect(sequenceToast("Revert", { kind: "Completed", applied: 1, skipped: [] })).toBe("Reverted 1 commit");
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
    expect(rebaseAbortToast({ restored: false }, null)).toBe(
      "Stopped the rebase. HEAD had moved, so it was left where it is.",
    );
  });
});
