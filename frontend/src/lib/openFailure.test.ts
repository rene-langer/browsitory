import { describe, expect, it, vi } from "vitest";
import { RepoError, type RepoClient } from "../ipc/RepoClient";
import { describeOpenFailure } from "./openFailure";

// A folder is "not a Git repository" only when it has no .git entry: its parent's one-level scan
// doesn't list it. Git refusing a folder that has one keeps Git's message (B3, #973).
function client(listed: Record<string, string[]>): RepoClient {
  return {
    listMissingRecentRepos: async () => [],
    scanReposInRoot: vi.fn(async (root: string) => listed[root] ?? []),
  } as unknown as RepoClient;
}

const refused = new RepoError("Other", "repository path 'C:\\work' is not owned by current user");

describe("describeOpenFailure", () => {
  it("keeps Git's message for a Windows folder its drive's scan lists", async () => {
    const failure = await describeOpenFailure(client({ "C:\\": ["C:\\work"] }), "C:\\work", refused);
    expect(failure).toEqual({ kind: "failed", path: "C:\\work", message: refused.message });
  });

  it("keeps Git's message for a nested Windows folder, a trailing separator aside", async () => {
    const failure = await describeOpenFailure(client({ "C:\\src": ["C:\\src\\work"] }), "C:\\src\\work\\", refused);
    expect(failure.kind).toBe("failed");
  });

  it("says a folder its parent's scan doesn't list isn't a repository", async () => {
    const failure = await describeOpenFailure(client({ "/home": [], "/home/work": ["/home/work/api"] }), "/home/work", refused);
    expect(failure).toEqual({ kind: "notRepository", path: "/home/work", message: refused.message, repos: ["/home/work/api"] });
  });
});
