import { describe, expect, it } from "vitest";
import { CloneError, toCloneError } from "./RepoClient";

describe("toCloneError", () => {
  it("keeps kind and message from a backend { kind, message } payload", () => {
    const error = toCloneError({ kind: "DestNotEmpty", message: "Destination already exists and isn't empty." });
    expect(error).toBeInstanceOf(CloneError);
    expect(error.kind).toBe("DestNotEmpty");
    expect(error.message).toBe("Destination already exists and isn't empty.");
  });

  it("maps anything else to a TransferFailed error with its string form", () => {
    const error = toCloneError("worker crashed");
    expect(error.kind).toBe("TransferFailed");
    expect(error.message).toBe("worker crashed");
  });
});
