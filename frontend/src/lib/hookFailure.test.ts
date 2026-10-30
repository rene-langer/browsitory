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

  it("offers Commit without hooks for a failed pre-merge-commit", () => {
    expect(
      retryLabel({ output: "", dialogOpen: true, hook: "pre-merge-commit", retry: { kind: "commit", message: "m", trailers: [] } }),
    ).toBe("Commit without hooks");
  });

  it("names the success toast after the original action", () => {
    expect(retrySuccessLabel({ kind: "commit", message: "m", trailers: [] })).toBe("Committed");
    expect(retrySuccessLabel({ kind: "push", push: { operation: "ForcePushBranch", remoteName: "o" } })).toBe("Force pushed");
    expect(retrySuccessLabel({ kind: "push", push: { operation: "PushTags", remoteName: "o", names: [] } })).toBe("Push complete");
    expect(
      retrySuccessLabel({ kind: "push", push: { operation: "DeleteRemoteBranch", remoteName: "o", branchName: "b" } }),
    ).toBe("Remote branch deleted");
  });
});
