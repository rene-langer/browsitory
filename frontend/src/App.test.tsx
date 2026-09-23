import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "./App";
import styles from "./App.module.css";
import type { RepoClient } from "./ipc/RepoClient";
import { publishTransportStatus } from "./ipc/transportStatus";

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
    getCommitMessage: unused,
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
    cancelTransfer: async () => unused(),
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
    logFrontendError: unused,
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

  it("leaves updater UI absent unless the transport bootstrap injects it", async () => {
    const listOpenRepos = vi.fn().mockResolvedValue({ entries: [], activePath: null });
    const client = fakeClient({ listOpenRepos });
    const view = render(<App client={client} />);
    await waitFor(() => expect(listOpenRepos).toHaveBeenCalledOnce());

    expect(screen.queryByText("Tauri updater")).not.toBeInTheDocument();

    view.rerender(
      <App client={client} updateBanner={<div>Tauri updater</div>} />,
    );

    expect(screen.getByText("Tauri updater")).toBeInTheDocument();
  });

  it("renders a transport status failure with the existing inline error treatment", async () => {
    const listOpenRepos = vi.fn().mockResolvedValue({ entries: [], activePath: null });
    const client = fakeClient({ listOpenRepos });

    render(<App client={client} />);
    await waitFor(() => expect(listOpenRepos).toHaveBeenCalledOnce());

    act(() => {
      publishTransportStatus({
        state: "failed",
        message: "sidecar stopped unexpectedly",
      });
    });

    expect(await screen.findByText("sidecar stopped unexpectedly")).toBeInTheDocument();
  });

  it("renders the header and a loading placeholder while the open repos restore", () => {
    const client = fakeClient({ listOpenRepos: () => new Promise(() => {}) });
    render(<App client={client} />);
    expect(screen.getByRole("heading", { name: "Browsitory" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Loading");
  });

  it("shows a failed open-repos restore in the reserved error slot instead of shifting the layout", async () => {
    const client = fakeClient({
      listOpenRepos: async () => {
        throw new Error("config.toml is unreadable");
      },
    });

    render(<App client={client} />);

    const banner = await screen.findByText("Error: config.toml is unreadable");
    // The reserved-space slot (`.errorLayer`, shared with the other App-level banners) floats
    // the banner over the workspace instead of pushing it down (FB-004) — asserting the class
    // instead of computed layout, since jsdom doesn't run layout.
    expect(banner.closest(`.${styles.errorLayer}`)).not.toBeNull();
  });

  it("closes the active tab on Ctrl/Cmd+W", async () => {
    const closeRepo = vi.fn().mockResolvedValue(undefined);
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      closeRepo,
      persistOpenRepos: async () => {},
      // RepoWorkspace's mount-time refresh() fans out to these; the fakeClient defaults for
      // several of them are `unused` (throw), which is fine for tests that never mount a
      // RepoWorkspace but this one does.
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    render(<App client={client} />);

    await screen.findByRole("tab", { name: "a" });
    expect(screen.getByRole("tab", { name: "b" })).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "w", ctrlKey: true });

    await waitFor(
      () => expect(screen.queryByRole("tab", { name: "a" })).not.toBeInTheDocument(),
      { timeout: 3000 },
    );
    expect(screen.getByRole("tab", { name: "b" })).toBeInTheDocument();
  });

  it("unmounts an inactive repo's workspace instead of hiding it with CSS (PERF-001)", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/b",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      // RepoWorkspace's mount-time refresh() fans out to these for whichever tab actually mounts.
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    render(<App client={client} />);

    await screen.findByRole("tab", { name: "a" });
    expect(screen.getByRole("tab", { name: "b" })).toBeInTheDocument();

    // Both tabs exist in the tab strip, but only the active repo's workspace (b) is mounted —
    // the audit's own evidence for this finding was three open repos producing three copies of
    // section headings like "Branches" in the DOM; the fix renders exactly one.
    await waitFor(() => expect(screen.queryAllByText("Branches")).toHaveLength(1));
  });
});
