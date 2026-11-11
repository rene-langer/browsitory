import { describe, expect, it } from "vitest";
import { hookFailedMessage, retryLabel, retrySuccessLabel, runningHookLabel } from "./hookFailure";

describe("hookFailure", () => {
  it("builds the copy", () => {
    expect(hookFailedMessage("pre-commit")).toBe("The pre-commit hook failed.");
    expect(runningHookLabel("pre-push")).toBe("Running pre-push hook…");
    expect(runningHookLabel("pre-commit")).toBe("Running pre-commit hook…");
  });

  it("labels the signing program, which is not a hook", () => {
    expect(runningHookLabel("signing")).toBe("Waiting for the signing program…");
  });

  it("offers a retry only for hooks --no-verify skips", () => {
    const base = { output: "", dialogOpen: true };
    expect(retryLabel({ ...base, hook: "pre-commit", retry: { kind: "commit", message: "m", trailers: [] } })).toBe("Commit without hooks");
    expect(retryLabel({ ...base, hook: "commit-msg", retry: { kind: "amend", message: "m", options: { author: null, trailers: [] } } })).toBe("Amend without hooks");
    expect(
      retryLabel({ ...base, hook: "pre-push", retry: { kind: "push", push: { operation: "PushBranch", remoteName: "o" } } }),
    ).toBe("Push without hooks");
    expect(retryLabel({ ...base, hook: "prepare-commit-msg", retry: { kind: "commit", message: "m", trailers: [] } })).toBeNull();
  });

  it("offers no retry when Apply as commits was rejected by a hook (#747)", () => {
    for (const hook of ["applypatch-msg", "pre-applypatch"]) {
      expect(retryLabel({ output: "", dialogOpen: true, hook, retry: { kind: "applyPatch" } })).toBeNull();
    }
  });

  it("offers Commit without hooks for a failed pre-merge-commit", () => {
    expect(
      retryLabel({ output: "", dialogOpen: true, hook: "pre-merge-commit", retry: { kind: "commit", message: "m", trailers: [] } }),
    ).toBe("Commit without hooks");
  });

  it("offers Continue without hooks after a failed hook on Continue", () => {
    expect(retryLabel({ output: "", dialogOpen: true, hook: "commit-msg", retry: { kind: "continueSequence" } })).toBe(
      "Continue without hooks",
    );
  });

  it("offers no retry for a failed prepare-commit-msg on Continue", () => {
    expect(
      retryLabel({ output: "", dialogOpen: true, hook: "prepare-commit-msg", retry: { kind: "continueSequence" } }),
    ).toBeNull();
  });

  it("names the success toast after the original action", () => {
    expect(retrySuccessLabel({ kind: "commit", message: "m", trailers: [] })).toBe("Committed");
    expect(retrySuccessLabel({ kind: "amend", message: "m", options: { author: null, trailers: [] } })).toBe("Amended commit");
  });
});
