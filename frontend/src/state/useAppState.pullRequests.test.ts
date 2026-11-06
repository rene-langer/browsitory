import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { PullRequest } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

describe("useAppState pull requests", () => {
  const remoteAPullRequest: PullRequest = {
    id: "a-1",
    number: 1,
    title: "Remote A's pull request",
    url: "https://github.com/acme/a/pull/1",
    author: "rene",
    sourceBranch: "feature/a",
    targetBranch: "main",
    state: "open",
  };
  const remoteBPullRequest: PullRequest = {
    id: "b-1",
    number: 2,
    title: "Remote B's pull request",
    url: "https://github.com/acme/b/pull/2",
    author: "rene",
    sourceBranch: "feature/b",
    targetBranch: "main",
    state: "open",
  };

  it("creating a pull request on remote B does not touch remote A's already-listed rows", async () => {
    const client = transferClient({
      listPullRequests: async (_repoPath: string, remoteName: string) => ({
        pullRequests: remoteName === "remote-a" ? [remoteAPullRequest] : [],
        truncated: false,
      }),
      createPullRequest: async () => remoteBPullRequest,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.listPullRequests("remote-a", "rene"));
    expect(result.current.state.pullRequests["remote-a"].pullRequests).toEqual([remoteAPullRequest]);

    await act(() =>
      result.current.createPullRequest("remote-b", "rene", {
        title: "Remote B's pull request",
        description: null,
        sourceBranch: "feature/b",
        targetBranch: "main",
      }),
    );

    expect(result.current.state.pullRequests["remote-a"].pullRequests).toEqual([remoteAPullRequest]);
    expect(result.current.state.pullRequests["remote-b"].pullRequests).toEqual([remoteBPullRequest]);
  });

  it("a failed listing on remote B does not show remote A's stale rows, and clears remote B's own entry", async () => {
    let failNextBListing = false;
    const client = transferClient({
      listPullRequests: async (_repoPath: string, remoteName: string) => {
        if (remoteName === "remote-b" && failNextBListing) {
          throw new Error("the provider rejected the request");
        }
        return {
          pullRequests: remoteName === "remote-a" ? [remoteAPullRequest] : [remoteBPullRequest],
          truncated: false,
        };
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.listPullRequests("remote-a", "rene"));
    await act(() => result.current.listPullRequests("remote-b", "rene"));
    expect(result.current.state.pullRequests["remote-a"].pullRequests).toEqual([remoteAPullRequest]);
    expect(result.current.state.pullRequests["remote-b"].pullRequests).toEqual([remoteBPullRequest]);

    failNextBListing = true;
    await act(() => result.current.listPullRequests("remote-b", "rene"));

    // Remote A's rows are untouched by remote B's failure...
    expect(result.current.state.pullRequests["remote-a"].pullRequests).toEqual([remoteAPullRequest]);
    // ...and remote B's own stale rows are cleared rather than left looking like a successful list.
    expect(result.current.state.pullRequests["remote-b"]).toBeUndefined();
    expect(result.current.state.error).toContain("the provider rejected the request");
  });
});
