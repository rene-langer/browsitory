import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
    supportsCloneAndInit: false,
    cloneRepo: async () => unused(),
    cancelClone: async () => unused(),
    initRepo: async () => unused(),
    listWorkspaces: async () => [],
    saveWorkspace: async () => "workspace-id",
    updateWorkspace: async () => {},
    deleteWorkspace: async () => {},
    getStatus: unused,
    getCommitGraph: unused,
    getGraphBranchSelection: async () => null,
    setGraphBranchSelection: unused,
    getGraphRemoteBranchSelection: async () => null,
    setGraphRemoteBranchSelection: unused,
    getWorkingDiff: async () => [],
    getCommitDiff: async () => [],
    getFileHistory: unused,
    searchCommits: unused,
    cancelSearch: async () => {},
    getCommitRenameDiff: unused,
    getCommitFiles: unused,
    getCommitMessage: unused,
    getCommitSignature: unused,
    stageFile: unused,
    unstageFile: unused,
    stageHunk: unused,
    unstageHunk: unused,
    discardHunk: unused,
    stageLines: unused,
    unstageLines: unused,
    discardLines: unused,
    discardFile: unused,
    discardAllUnstaged: unused,
    ignorePath: unused,
    getDiffSettings: unused,
    setDiffSettings: unused,
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
    fetchAllRemotes: async () => unused(),
    pushBranch: async () => unused(),
    deleteRemoteBranch: async () => unused(),
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
    getHeadInfo: async () => ({ headId: null, detachedAt: null, headPushed: false, operation: null }),
    amendCommit: async () => { throw new Error("unimplemented"); },
    getUndoState: async () => ({ undo: null, redo: null }),
    undo: async () => ({ kind: "Nothing" as const }),
    redo: async () => ({ kind: "Nothing" as const }),
    cherryPick: async () => { throw new Error("unimplemented"); },
    revertCommit: async () => { throw new Error("unimplemented"); },
    resetToCommit: async () => { throw new Error("unimplemented"); },
    checkoutCommit: async () => { throw new Error("unimplemented"); },
    forcePushCurrentBranch: async () => { throw new Error("unimplemented"); },
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

  it("renders a single Notifications region with a repository open", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });
    render(<App client={client} />);
    await screen.findByRole("tab", { name: "a" });
    expect(screen.getAllByRole("status", { name: "Notifications" })).toHaveLength(1);
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

  describe("palette discard commands", () => {
    function discardClient(overrides: Partial<RepoClient> = {}) {
      return fakeClient({
        listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
        openRepo: async () => {},
        persistOpenRepos: async () => {},
        getStatus: async () => [{ path: "a.txt", staged: false, kind: "Modified" }],
        getCommitGraph: async () => [],
        listBranches: async () => [],
        listStashes: async () => [],
        getDiffSettings: async () => ({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true }),
        discardFile: vi.fn().mockResolvedValue(undefined),
        discardAllUnstaged: vi.fn().mockResolvedValue(undefined),
        ...overrides,
      });
    }

    async function runPaletteCommand(label: string) {
      fireEvent.keyDown(window, { key: "k", ctrlKey: true });
      const input = await screen.findByRole("combobox", { name: "Command palette" });
      fireEvent.change(input, { target: { value: label } });
      fireEvent.click(await screen.findByRole("option", { name: label }));
    }

    it("opens the file confirm without discarding, then cancels and confirms", async () => {
      const client = discardClient();
      render(<App client={client} />);
      fireEvent.click(await screen.findByText("a.txt (Modified)"));

      await runPaletteCommand("Discard changes in current file");
      expect(await screen.findByText("Discard changes to a.txt? This can't be undone.")).toBeInTheDocument();
      expect(client.discardFile).not.toHaveBeenCalled();

      fireEvent.click(screen.getByText("Cancel"));
      expect(screen.queryByText("Discard changes to a.txt? This can't be undone.")).not.toBeInTheDocument();
      expect(client.discardFile).not.toHaveBeenCalled();

      await runPaletteCommand("Discard changes in current file");
      fireEvent.click(await screen.findByRole("button", { name: "Discard changes" }));
      await waitFor(() => expect(client.discardFile).toHaveBeenCalledTimes(1));
      expect(client.discardFile).toHaveBeenCalledWith("/repos/a", "a.txt");
    });

    it("opens the discard-all confirm without discarding, then cancels and confirms", async () => {
      const client = discardClient();
      render(<App client={client} />);
      await screen.findByText("a.txt (Modified)");

      await runPaletteCommand("Discard all unstaged changes");
      expect(await screen.findByText("Discard changes to 1 file? This can't be undone.")).toBeInTheDocument();
      expect(client.discardAllUnstaged).not.toHaveBeenCalled();

      fireEvent.click(screen.getByText("Cancel"));
      expect(screen.queryByText("Discard changes to 1 file? This can't be undone.")).not.toBeInTheDocument();
      expect(client.discardAllUnstaged).not.toHaveBeenCalled();

      await runPaletteCommand("Discard all unstaged changes");
      fireEvent.click(await screen.findByRole("button", { name: "Discard all" }));
      await waitFor(() => expect(client.discardAllUnstaged).toHaveBeenCalledTimes(1));
      expect(client.discardAllUnstaged).toHaveBeenCalledWith("/repos/a", ["a.txt"]);
    });
  });

  it("closes a focused, inactive tab on Delete without switching away from the active one", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      closeRepo: vi.fn().mockResolvedValue(undefined),
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    render(<App client={client} />);
    const tabB = await screen.findByRole("tab", { name: "b" });
    tabB.focus();

    fireEvent.keyDown(tabB, { key: "Delete" });

    await waitFor(() => expect(screen.queryByRole("tab", { name: "b" })).not.toBeInTheDocument());
    expect(screen.getByRole("tab", { name: "a" })).toHaveAttribute("aria-selected", "true");
    expect(client.closeRepo).toHaveBeenCalledWith("/repos/b");
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

  it("keeps a half-typed commit message across switching tabs away and back", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    render(<App client={client} />);
    await screen.findByRole("tab", { name: "a" });

    fireEvent.change(await screen.findByRole("textbox", { name: "Commit message" }), {
      target: { value: "wip: half-typed" },
    });

    fireEvent.click(screen.getByRole("tab", { name: "b" }));
    // b's workspace replaced a's (PERF-001 unmounts inactive tabs), with its own empty draft.
    await waitFor(() => expect(screen.getByRole("tab", { name: "b" })).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByRole("textbox", { name: "Commit message" })).toHaveValue("");

    fireEvent.click(screen.getByRole("tab", { name: "a" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "a" })).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByRole("textbox", { name: "Commit message" })).toHaveValue("wip: half-typed");
  });

  it("re-enables a tab's close button once it is switched away from mid-operation", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "x.txt", staged: false, kind: "Modified" }],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      // Never settles: the stash stays in flight, so repo a stays busy for as long as its
      // workspace is mounted.
      saveStash: () => new Promise(() => {}),
    });

    render(<App client={client} />);
    const tabA = await screen.findByRole("tab", { name: "a" });
    // The per-tab close button is `aria-hidden` (mouse-only), so it's found by position: the
    // tab's sibling inside its `role="presentation"` wrapper.
    const closeA = () => tabA.parentElement!.querySelector<HTMLButtonElement>("button:not([role='tab'])")!;

    const stash = await screen.findByRole("button", { name: "Stash" });
    await waitFor(() => expect(stash).toBeEnabled());
    fireEvent.click(stash);
    await waitFor(() => expect(closeA()).toBeDisabled());

    fireEvent.click(screen.getByRole("tab", { name: "b" }));

    await waitFor(() => expect(closeA()).toBeEnabled());
  });

  it("keeps a tab's close button disabled after it's switched away from mid-rebase, unlike a plain pending/transfer busy state", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "x.txt", staged: false, kind: "Modified" }],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      // Rebase state lives only in the worker's in-memory `rebase_state` (nothing but
      // `set_head_detached` is persisted), so unlike `pending`/`transfer` it must NOT be
      // reported as clear just because this tab's workspace unmounted.
      getRebaseProgress: async () => ({ currentStep: 1, totalSteps: 3 }),
    });

    render(<App client={client} />);
    const tabA = await screen.findByRole("tab", { name: "a" });
    const closeA = () => tabA.parentElement!.querySelector<HTMLButtonElement>("button:not([role='tab'])")!;

    await waitFor(() => expect(closeA()).toBeDisabled());

    fireEvent.click(screen.getByRole("tab", { name: "b" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "b" })).toHaveAttribute("aria-selected", "true"));

    // Unlike the plain pending/transfer case above, this stays disabled: `a`'s `RepoWorkspace`
    // unmounted while `rebaseProgress !== null`, and dropping that worker mid-rebase would leave
    // the repo with a detached HEAD and a conflicted index with no way to Continue or Abort.
    expect(closeA()).toBeDisabled();
  });

  it("opens the shortcut sheet on ? outside of a text input", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [{ path: "/repos/a", workspaceId: null }],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    render(<App client={client} />);
    await screen.findByRole("tab", { name: "a" });

    fireEvent.keyDown(window, { key: "?" });

    expect(screen.getByRole("heading", { name: "Keyboard shortcuts" })).toBeInTheDocument();
  });

  it("does not open the shortcut sheet on ? while typing in a text field", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [{ path: "/repos/a", workspaceId: null }],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    render(<App client={client} />);
    await screen.findByRole("tab", { name: "a" });

    // Create a temporary input and fire the event on it
    const input = document.createElement("input");
    document.body.appendChild(input);
    fireEvent.keyDown(input, { key: "?" });
    document.body.removeChild(input);

    expect(screen.queryByRole("heading", { name: "Keyboard shortcuts" })).not.toBeInTheDocument();
  });

  it("collapses the sidebar automatically below the narrow-window breakpoint", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [{ path: "/repos/a", workspaceId: null }],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    // Start with a wide window (above NARROW_BREAKPOINT of 900)
    const originalInnerWidth = window.innerWidth;
    Object.defineProperty(window, "innerWidth", {
      writable: true,
      configurable: true,
      value: 1200,
    });

    render(<App client={client} />);
    await screen.findByRole("tab", { name: "a" });
    await waitFor(() => expect(screen.queryAllByText("Branches")).toHaveLength(1));

    // The sidebar pane is the previous sibling of the divider (separator).
    // It has hidden={shownWidth === 0} where shownWidth depends on forceCollapsed.
    const divider = screen.getByRole("separator", { name: "Sidebar width" });
    const sidebarPane = divider.previousElementSibling as HTMLElement;

    // Sidebar should not be hidden when above breakpoint
    expect(sidebarPane).not.toHaveAttribute("hidden");

    // Narrow the window below NARROW_BREAKPOINT
    window.innerWidth = 800;
    act(() => {
      fireEvent(window, new Event("resize"));
    });

    // Sidebar should be hidden when below breakpoint
    await waitFor(() => {
      expect(sidebarPane).toHaveAttribute("hidden");
    });

    // Restore original width
    Object.defineProperty(window, "innerWidth", {
      writable: true,
      configurable: true,
      value: originalInnerWidth,
    });
  });

  it("publishes the current branch from the sync bar through the Push dialog", async () => {
    const pushBranch = vi.fn(async () => "push-1");
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [{ name: "main", isCurrent: true, upstream: null, ahead: null, behind: null }],
      listStashes: async () => [],
      listRemotes: async () => [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null, prune: false }],
      getCurrentUpstream: async () => null,
      pushBranch,
    });
    render(<App client={client} />);

    const push = await screen.findByRole("button", { name: "Push" });
    await waitFor(() => expect(push).toBeEnabled());
    fireEvent.click(push);
    const dialog = await screen.findByRole("dialog", { name: "Push main" });
    expect(within(dialog).getByLabelText("Set as upstream")).toBeChecked();
    fireEvent.click(within(dialog).getByRole("button", { name: "Push" }));

    await waitFor(() => expect(pushBranch).toHaveBeenCalledWith("/repos/a", "origin", "main", "main", true));
    expect(screen.queryByRole("dialog", { name: "Push main" })).toBeNull();
  });

  it("deletes a remote branch from its menu through the confirmation dialog", async () => {
    localStorage.clear();
    const deleteRemoteBranch = vi.fn(async () => "delete-remote-1");
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [{ name: "main", isCurrent: true }],
      listStashes: async () => [],
      listRemotes: async () => [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null }],
      listRemoteBranches: async () => ["topic"],
      deleteRemoteBranch,
    });
    render(<App client={client} />);

    fireEvent.click(await screen.findByRole("button", { name: "origin" }));
    fireEvent.click(await screen.findByRole("button", { name: "Actions for topic" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete origin/topic…" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete remote branch" });
    expect(dialog).toHaveTextContent("Delete origin/topic on the remote? This can't be undone.");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete remote branch" }));

    await waitFor(() => expect(deleteRemoteBranch).toHaveBeenCalledWith("/repos/a", "origin", "topic"));
    expect(screen.queryByRole("dialog", { name: "Delete remote branch" })).toBeNull();
    localStorage.clear();
  });

  it("opens a working-tree file's history in place of the graph and closes it again", async () => {
    const graphCommit = {
      id: "c2",
      shortId: "c2",
      summary: "edit a",
      authorName: "A",
      authorEmail: "a@example.com",
      timestamp: 1_700_000_000,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
    };
    const getFileHistory = vi.fn(async () => [{ commit: graphCommit, path: "a.txt", oldPath: null }]);
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "a.txt", staged: false, kind: "Modified" }],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      getFileHistory,
      getCommitFiles: async () => ["a.txt"],
      getCommitMessage: async () => "edit a",
    });

    render(<App client={client} />);
    fireEvent.click(await screen.findByRole("button", { name: "History of a.txt" }));

    expect(await screen.findByRole("toolbar", { name: "File history" })).toHaveTextContent("History: a.txt");
    expect(screen.queryByRole("listbox", { name: "Commit history" })).toBeNull();
    expect(getFileHistory).toHaveBeenCalledWith("/repos/a", "a.txt", 300);
    expect(await screen.findByRole("option", { selected: true })).toHaveTextContent("edit a");

    fireEvent.click(screen.getByRole("button", { name: "Close file history" }));
    expect(await screen.findByRole("listbox", { name: "Commit history" })).toBeInTheDocument();
  });

  it("searches from the graph with Ctrl+F, replaces the graph with results, and × restores it", async () => {
    const hit = {
      id: "s1",
      shortId: "s1",
      summary: "search hit",
      authorName: "Searchy McAuthor",
      authorEmail: "searchy@example.com",
      timestamp: 1_700_000_000,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
    };
    const searchCommits = vi.fn(async () => ({ commits: [hit], nextCursor: null, scanned: 1 }));
    const cancelSearch = vi.fn(async () => {});
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      getGraphBranchSelection: async () => ["main"],
      getCommitFiles: async () => [],
      getCommitMessage: async () => "search hit",
      getCommitSignature: async () => ({ status: "unsigned", format: null, signer: null, key: null, detail: null }),
      searchCommits,
      cancelSearch,
    });

    render(<App client={client} />);
    const graph = await screen.findByRole("listbox", { name: "Commit history" });
    graph.focus();
    fireEvent.keyDown(graph, { key: "f", ctrlKey: true });
    const input = screen.getByRole("searchbox", { name: "Search commits" });
    await waitFor(() => expect(input).toHaveFocus());

    fireEvent.change(input, { target: { value: "author:searchy" } });
    const results = await screen.findByRole("listbox", { name: "Search results" }, { timeout: 2000 });
    await waitFor(() => expect(searchCommits).toHaveBeenCalledWith("/repos/a", "author:searchy", ["main"], null));
    expect(screen.queryByRole("listbox", { name: "Commit history" })).toBeNull();
    expect(await within(results).findByRole("option", { selected: true })).toHaveTextContent("search hit");

    fireEvent.click(screen.getByRole("button", { name: "Close search" }));
    expect(await screen.findByRole("listbox", { name: "Commit history" })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: "Search commits" })).toHaveValue("");
  });

  describe("undo/redo", () => {
    function undoClient(overrides: Partial<RepoClient> = {}) {
      return fakeClient({
        listOpenRepos: async () => ({ entries: [{ path: "/repo", workspaceId: null }], activePath: "/repo" }),
        openRepo: async () => {},
        persistOpenRepos: async () => {},
        getStatus: async () => [],
        getCommitGraph: async () => [],
        listBranches: async () => [],
        listStashes: async () => [],
        ...overrides,
      });
    }

    it("undoes with Mod+Z and offers Redo in the toast", async () => {
      const undo = vi.fn(async () => ({ kind: "Done" as const, label: "Stage a.txt" }));
      const redo = vi.fn(async () => ({ kind: "Done" as const, label: "Stage a.txt" }));
      render(<App client={undoClient({ getUndoState: async () => ({ undo: "Stage a.txt", redo: null }), undo, redo })} />);
      await screen.findByRole("button", { name: "Undo Stage a.txt" });
      fireEvent.keyDown(window, { key: "z", ctrlKey: true });
      await waitFor(() => expect(undo).toHaveBeenCalled());
      expect(undo).toHaveBeenCalledWith("/repo");
      fireEvent.click(await screen.findByRole("button", { name: "Redo" }));
      expect(redo).toHaveBeenCalledWith("/repo");
      expect(await screen.findByText("Redid Stage a.txt")).toBeInTheDocument();
    });

    it("redoes with Mod+Shift+Z and Ctrl+Y", async () => {
      const redo = vi.fn(async () => ({ kind: "Nothing" as const }));
      render(<App client={undoClient({ getUndoState: async () => ({ undo: null, redo: "Stage a.txt" }), redo })} />);
      await screen.findByRole("button", { name: "Redo Stage a.txt" });
      fireEvent.keyDown(window, { key: "Z", ctrlKey: true, shiftKey: true });
      await waitFor(() => expect(redo).toHaveBeenCalledTimes(1));
      // A second press while the first is still settling is ignored (pending), so wait it out.
      await waitFor(() => {
        fireEvent.keyDown(window, { key: "y", ctrlKey: true });
        expect(redo).toHaveBeenCalledTimes(2);
      });
    });

    it("ignores Mod+Z while typing in the commit message", async () => {
      const undo = vi.fn(async () => ({ kind: "Nothing" as const }));
      render(<App client={undoClient({ getUndoState: async () => ({ undo: "Stage a.txt", redo: null }), undo })} />);
      await screen.findByRole("button", { name: "Undo Stage a.txt" });
      const message = await screen.findByRole("textbox", { name: /commit message/i });
      message.focus();
      expect(message).toHaveFocus();
      fireEvent.keyDown(message, { key: "z", ctrlKey: true });
      expect(undo).not.toHaveBeenCalled();
    });

    it("a toast's Redo does nothing once its repo tab was switched away", async () => {
      const undo = vi.fn(async () => ({ kind: "Done" as const, label: "Stage a.txt" }));
      const redo = vi.fn(async () => ({ kind: "Done" as const, label: "Stage a.txt" }));
      render(
        <App
          client={undoClient({
            listOpenRepos: async () => ({
              entries: [
                { path: "/repos/a", workspaceId: null },
                { path: "/repos/b", workspaceId: null },
              ],
              activePath: "/repos/a",
            }),
            getUndoState: async () => ({ undo: "Stage a.txt", redo: null }),
            undo,
            redo,
          })}
        />,
      );
      await screen.findByRole("button", { name: "Undo Stage a.txt" });
      fireEvent.keyDown(window, { key: "z", ctrlKey: true });
      const toastRedo = await screen.findByRole("button", { name: "Redo" });
      fireEvent.click(screen.getByRole("tab", { name: "b" }));
      await waitFor(() => expect(screen.getByRole("tab", { name: "b" })).toHaveAttribute("aria-selected", "true"));
      fireEvent.click(toastRedo);
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(redo).not.toHaveBeenCalled();
    });

    it("re-reads the undo state when the window regains focus", async () => {
      const getUndoState = vi.fn(async () => ({ undo: null, redo: null }));
      render(<App client={undoClient({ getUndoState })} />);
      await screen.findByRole("button", { name: "Nothing to undo" });
      const calls = getUndoState.mock.calls.length;
      window.dispatchEvent(new Event("focus"));
      await waitFor(() => expect(getUndoState.mock.calls.length).toBe(calls + 1));
    });
  });
});
