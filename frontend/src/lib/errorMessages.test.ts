import { describe, expect, it } from "vitest";
import { RepoError } from "../ipc/RepoClient";
import { describeError } from "./errorMessages";

describe("describeError", () => {
  it("adds a hint when the VSCode extension lost its sidecar", () => {
    const result = describeError(new RepoError("TransportLost", "Browsitory sidecar exited with code 1"));
    expect(result.message).toBe("Browsitory sidecar exited with code 1");
    expect(result.hint).toBe("The connection to the backend was lost. Retry, or reopen the repository.");
  });

  // #496: the hint follows the kind; a message that merely mentions the sidecar gets none.
  it("adds no hint for another kind, whatever its text", () => {
    const result = describeError(new RepoError("Other", "Browsitory sidecar exited with code 1"));
    expect(result.hint).toBeUndefined();
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
