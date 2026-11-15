import { describe, expect, it } from "vitest";
import { RepoError } from "../ipc/RepoClient";
import { describeError } from "./errorMessages";

describe("describeError", () => {
  it("adds a hint when the VSCode extension lost its sidecar", () => {
    const result = describeError(new RepoError("TransportLost", "Browsitory sidecar exited with code 1"));
    expect(result.message).toBe("Browsitory sidecar exited with code 1");
    expect(result.hint).toBe("The connection to the backend was lost. Retry, or reopen the repository.");
  });

  // #527: the OS refused a file access; the kind comes from the backend's typed error, not
  // from the text ("Permission denied" here, "Access is denied." on Windows).
  it("adds a hint when file permissions blocked the request", () => {
    const result = describeError(new RepoError("PermissionDenied", "failed to open 'a.txt': Permission denied"));
    expect(result.message).toBe("failed to open 'a.txt': Permission denied");
    expect(result.hint).toBe("File permissions blocked this. Check that you can read and write the file or folder.");
  });

  // #663: every kind with a next step has a hint; those whose message is the remediation don't.
  it.each([
    ["DirtyWorktree", "Commit or stash your changes, then try again."],
    ["PullFailed", "Check the remote and your connection, then pull again."],
    ["Cancelled", "It was cancelled before it finished. Run it again when you are ready."],
    ["AddDeleteConflict", "One side deleted the file: keep it or delete it to resolve the conflict."],
    ["InvalidSearchQuery", "Check the search for a typo, or a branch, tag or path that doesn't exist."],
    ["LocalChanges", "Commit or stash the changes to those files first, or use Stash and switch."],
    ["MissingIdentity", "Set your name and email for commits, then commit again."],
  ] as const)("adds a hint for %s", (kind, hint) => {
    expect(describeError(new RepoError(kind, "raw backend text"))).toEqual({ message: "raw backend text", hint });
  });

  it.each(["WorkerStopped", "CredentialStoreFailure", "Other"] as const)("adds no hint for %s", (kind) => {
    expect(describeError(new RepoError(kind, "text")).hint).toBeUndefined();
  });

  it("adds no permission hint to a message that only mentions it", () => {
    expect(describeError(new RepoError("Other", "Permission denied")).hint).toBeUndefined();
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

// #997: libgit2's class and code are for bug reports, not for the banner.
describe("withoutLibgit2Codes", () => {
  it.each([
    [
      "reference 'refs/heads/main' not found; class=Reference (4); code=UnbornBranch (-9)",
      "reference 'refs/heads/main' not found",
    ],
    [
      "failed to resolve path '/gone': No such file or directory; class=Os (2); code=NotFound (-3)",
      "failed to resolve path '/gone': No such file or directory",
    ],
    ["could not find repository at '/work'; class=Repository (6); code=NotFound (-3)", "could not find repository at '/work'"],
    ["failed to connect; class=Net (12)", "failed to connect"],
    ["object not found; code=NotFound (-3)", "object not found"],
    ["Commit or unstage your staged changes first.", "Commit or unstage your staged changes first."],
    ["git operation failed: bad; class=Index (10); code=Conflict (-13). Fetch first.", "git operation failed: bad. Fetch first."],
  ])("turns %j into %j", (raw, shown) => {
    expect(withoutLibgit2Codes(raw)).toBe(shown);
  });
});
