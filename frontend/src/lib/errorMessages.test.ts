import { describe, expect, it } from "vitest";
import { describeError } from "./errorMessages";

describe("describeError", () => {
  it("adds a hint for a sidecar transport failure", () => {
    const result = describeError(new Error("Transport failed: sidecar exited unexpectedly (code 1)"));
    expect(result.message).toBe("Transport failed: sidecar exited unexpectedly (code 1)");
    expect(result.hint).toBe("The connection to the backend was lost. Retry, or reopen the repository.");
  });

  it("adds a hint for a working-diff fetch failure", () => {
    const result = describeError(new Error("failed to read working diff: permission denied"));
    expect(result.hint).toBe("Could not read this file's changes. Retry, or check the file still exists.");
  });

  it("falls back to no hint for an unrecognized error", () => {
    const result = describeError(new Error("something unusual"));
    expect(result.message).toBe("something unusual");
    expect(result.hint).toBeUndefined();
  });

  it("stringifies a non-Error thrown value as the message", () => {
    const result = describeError("plain string failure");
    expect(result.message).toBe("plain string failure");
    expect(result.hint).toBeUndefined();
  });
});
