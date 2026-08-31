import { act, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "./App";
import type { RepoClient } from "./ipc/RepoClient";

function unused(): never {
  throw new Error("not used in this test");
}

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return {
    pickRepoFolder: unused,
    listRecentRepos: async () => [],
    getAppVersion: async () => "0.0.0",
    getLastSeenVersion: async () => "0.0.0",
    setLastSeenVersion: async () => {},
    openRepo: async () => {},
    closeRepo: async () => unused(),
    listOpenRepos: async () => ({ entries: [], activePath: null }),
    persistOpenRepos: async () => unused(),
    scanReposInRoot: async () => [],
    listWorkspaces: async () => [],
    saveWorkspace: async () => "workspace-id",
    updateWorkspace: async () => {},
    deleteWorkspace: async () => {},
    getStatus: unused,
    getCommitGraph: unused,
    getGraphBranchSelection: async () => null,
    setGraphBranchSelection: unused,
    getWorkingDiff: async () => [],
    getCommitDiff: async () => [],
    getCommitFiles: unused,
    stageFile: unused,
    unstageFile: unused,
    stageHunk: unused,
    unstageHunk: unused,
    discardHunk: unused,
    commit: unused,
    listBranches: unused,
    createBranch: unused,
    switchBranch: unused,
    deleteBranch: unused,
    renameBranch: unused,
    listWorktrees: async () => [],
    createWorktree: async () => unused(),
    removeWorktree: async () => unused(),
    pruneWorktrees: async () => unused(),
    listSubmodules: async () => [],
    initSubmodule: async () => unused(),
    updateSubmodule: async () => unused(),
    listReflogRefs: async () => [],
    getReflog: async () => [],
    restoreReflogEntry: async () => unused(),
    listRemotes: async () => [],
    listRemoteBranches: async () => [],
    getCurrentUpstream: async () => null,
    getRemoteUpstreams: async () => [],
    addRemote: async () => unused(),
    renameRemote: async () => unused(),
    updateRemoteUrls: async () => unused(),
    removeRemote: async () => unused(),
    saveHttpsCredential: async () => unused(),
    forgetHttpsCredential: async () => unused(),
    setRemoteAuthMode: async () => unused(),
    setCurrentUpstream: async () => unused(),
    clearCurrentUpstream: async () => unused(),
    listTags: async () => [],
    createTag: async () => unused(),
    deleteTag: async () => unused(),
    fetchRemote: async () => unused(),
    pushCurrentBranch: async () => unused(),
    pushTags: async () => unused(),
    pullCurrentUpstream: async () => unused(),
    subscribeTransferProgress: () => () => {},
    listStashes: unused,
    saveStash: unused,
    applyStash: unused,
    dropStash: unused,
    getBlame: unused,
    mergeBranch: unused,
    getConflictHunks: unused,
    resolveConflict: unused,
    abortMerge: unused,
    getMergeMessage: async () => null,
    resolveAddDeleteConflict: unused,
    commitsSince: unused,
    startRebase: unused,
    rebaseContinue: unused,
    abortRebase: unused,
    getRebaseProgress: async () => null,
    detectForgeRepository: async () => [],
    saveForgeToken: unused,
    forgetForgeToken: unused,
    listPullRequests: unused,
    createPullRequest: unused,
    openExternalUrl: unused,
    ...overrides,
  };
}

describe("App", () => {
  it("uses its injected RepoClient to restore the initial repositories", async () => {
    const listOpenRepos = vi.fn().mockResolvedValue({ entries: [], activePath: null });
    const client = fakeClient({ listOpenRepos });

    render(<App client={client} />);

    await waitFor(() => expect(listOpenRepos).toHaveBeenCalledOnce());
  });

  it("renders a transport status failure with the existing inline error treatment", async () => {
    const listOpenRepos = vi.fn().mockResolvedValue({ entries: [], activePath: null });
    const client = fakeClient({ listOpenRepos });

    render(<App client={client} />);
    await waitFor(() => expect(listOpenRepos).toHaveBeenCalledOnce());

    act(() => {
      window.dispatchEvent(new MessageEvent("message", {
        data: {
          jsonrpc: "2.0",
          method: "transportStatus",
          params: { state: "failed", message: "sidecar stopped unexpectedly" },
        },
      }));
    });

    expect(await screen.findByText("sidecar stopped unexpectedly")).toBeInTheDocument();
  });
});
