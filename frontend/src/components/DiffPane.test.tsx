import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { BlameLine, ConflictSegment, DiffHunk, RepoClient, StatusEntry } from "../ipc/RepoClient";
import { DiffPane } from "./DiffPane";

const TEST_REPO_PATH = "/repo";

function unused(): never {
  throw new Error("not used in this test");
}

function fakeClient(overrides: Partial<RepoClient>): RepoClient {
  return {
    pickRepoFolder: unused,
    listRecentRepos: unused,
    getAppVersion: unused,
    getLastSeenVersion: unused,
    setLastSeenVersion: unused,
    openRepo: unused,
    closeRepo: async () => unused(),
    listOpenRepos: async () => unused(),
    persistOpenRepos: async () => unused(),
    scanReposInRoot: async () => [],
    listWorkspaces: async () => [],
    saveWorkspace: async () => "workspace-id",
    updateWorkspace: async () => {},
    deleteWorkspace: async () => {},
    getStatus: unused,
    getCommitGraph: unused,
    listBranches: unused,
    createBranch: unused,
    switchBranch: unused,
    deleteBranch: unused,
    renameBranch: unused,
    listWorktrees: async () => unused(),
    createWorktree: async () => unused(),
    removeWorktree: async () => unused(),
    pruneWorktrees: async () => unused(),
    listSubmodules: async () => [],
    initSubmodule: async () => unused(),
    updateSubmodule: async () => unused(),
    listReflogRefs: async () => [],
    getReflog: async () => [],
    restoreReflogEntry: async () => unused(),
    listRemotes: async () => unused(),
    listRemoteBranches: async () => unused(),
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
    getGraphBranchSelection: async () => null,
    setGraphBranchSelection: unused,
    getWorkingDiff: unused,
    getCommitDiff: unused,
    getCommitFiles: unused,
    stageFile: unused,
    unstageFile: unused,
    stageHunk: unused,
    unstageHunk: unused,
    discardHunk: unused,
    commit: unused,
    ...overrides,
  };
}

describe("DiffPane", () => {
  describe("uncommitted", () => {
    const status: StatusEntry[] = [
      { path: "a.txt", staged: false, kind: "Modified" },
      { path: "b.txt", staged: true, kind: "New" },
    ];

    it("renders an icon-only Stage control for unstaged entries and Unstage for staged ones", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getByRole("button", { name: "Stage a.txt" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Unstage b.txt" })).toBeInTheDocument();
    });

    it("shows Stage Hunk (not Unstage Hunk) and Discard Hunk for an unstaged file's diff", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      const client = fakeClient({ getWorkingDiff: async () => hunks });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("a.txt (Modified)"));

      expect(await screen.findByText("Stage Hunk")).toBeInTheDocument();
      expect(screen.queryByText("Unstage Hunk")).not.toBeInTheDocument();
      expect(screen.getByText("Discard Hunk")).toBeInTheDocument();
    });

    it("shows Unstage Hunk (not Stage Hunk) and Discard Hunk for a staged file's diff", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      const client = fakeClient({ getWorkingDiff: async () => hunks });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("b.txt (New)"));

      expect(await screen.findByText("Unstage Hunk")).toBeInTheDocument();
      expect(screen.queryByText("Stage Hunk")).not.toBeInTheDocument();
      expect(screen.getByText("Discard Hunk")).toBeInTheDocument();
    });

    it("clicking Stage Hunk calls onStageHunk with the selected file's path and the hunk's start lines", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 3, oldLines: 1, newStart: 4, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      const client = fakeClient({ getWorkingDiff: async () => hunks });
      const onStageHunk = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={onStageHunk}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("a.txt (Modified)"));
      fireEvent.click(await screen.findByText("Stage Hunk"));

      expect(onStageHunk).toHaveBeenCalledWith("a.txt", 3, 4);
    });

    it("clicking the Stage control calls onStageFile with that path", () => {
      const client = fakeClient({});
      const onStageFile = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={onStageFile}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByRole("button", { name: "Stage a.txt" }));

      expect(onStageFile).toHaveBeenCalledWith("a.txt");
    });

    // One bulk call, not one per file: each per-file call is a separate `runMutation` in
    // `useAppState`, and every `runMutation` ends in a full `refresh()` (~13 IPC reads plus one
    // per remote), all serialized through the single per-repo worker thread.
    it("Stage all makes a single bulk call with every unstaged path", () => {
      const threeStatus: StatusEntry[] = [
        { path: "a.txt", staged: false, kind: "Modified" },
        { path: "c.txt", staged: false, kind: "New" },
        { path: "b.txt", staged: true, kind: "New" },
      ];
      const client = fakeClient({});
      const onStageFile = vi.fn();
      const onStageAllFiles = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={threeStatus}
          onStageFile={onStageFile}
          onUnstageFile={vi.fn()}
          onStageAllFiles={onStageAllFiles}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByRole("button", { name: "Stage all" }));

      expect(onStageAllFiles).toHaveBeenCalledWith(["a.txt", "c.txt"]);
      expect(onStageAllFiles).toHaveBeenCalledTimes(1);
      expect(onStageFile).not.toHaveBeenCalled();
    });

    // Staging a conflicted path is what marks the conflict resolved (with whatever is in the
    // working tree). One "Stage all" click must not silently resolve an outstanding merge.
    it("Stage all omits Conflicted entries from the bulk call", () => {
      const conflictedMix: StatusEntry[] = [
        { path: "a.txt", staged: false, kind: "Modified" },
        { path: "shared.txt", staged: false, kind: "Conflicted" },
      ];
      const client = fakeClient({});
      const onStageAllFiles = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={conflictedMix}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={onStageAllFiles}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByRole("button", { name: "Stage all" }));

      expect(onStageAllFiles).toHaveBeenCalledWith(["a.txt"]);
      // The conflicted file keeps its own per-row Stage control — only the bulk action skips it.
      expect(screen.getByRole("button", { name: "Stage shared.txt" })).toBeInTheDocument();
    });

    it("disables Stage all when every unstaged entry is Conflicted", () => {
      const conflictsOnly: StatusEntry[] = [
        { path: "shared.txt", staged: false, kind: "Conflicted" },
      ];
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={conflictsOnly}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getByRole("button", { name: "Stage all" })).toBeDisabled();
    });

    it("Unstage all makes a single bulk call with every staged path", () => {
      const client = fakeClient({});
      const onUnstageFile = vi.fn();
      const onUnstageAllFiles = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={onUnstageFile}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={onUnstageAllFiles}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByRole("button", { name: "Unstage all" }));

      expect(onUnstageAllFiles).toHaveBeenCalledWith(["b.txt"]);
      expect(onUnstageAllFiles).toHaveBeenCalledTimes(1);
      expect(onUnstageFile).not.toHaveBeenCalled();
    });

    it("does not render Stage all when there are no unstaged entries", () => {
      const stagedOnly: StatusEntry[] = [{ path: "b.txt", staged: true, kind: "New" }];
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={stagedOnly}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.queryByRole("button", { name: "Stage all" })).not.toBeInTheDocument();
    });

    it("clicking a file fetches and renders its working diff", async () => {
      const hunks: DiffHunk[] = [
        {
          oldStart: 1,
          oldLines: 1,
          newStart: 1,
          newLines: 1,
          lines: [{ origin: "Add", content: "changed line" }],
        },
      ];
      const getWorkingDiff = vi.fn(async () => hunks);
      const client = fakeClient({ getWorkingDiff });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("a.txt (Modified)"));

      expect(await screen.findByText(/changed line/)).toBeInTheDocument();
      expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "a.txt", false);
    });

    it("clicking a staged file's diff button fetches the staged diff", async () => {
      const hunks: DiffHunk[] = [
        {
          oldStart: 1,
          oldLines: 1,
          newStart: 1,
          newLines: 1,
          lines: [{ origin: "Add", content: "staged content" }],
        },
      ];
      const getWorkingDiff = vi.fn(async () => hunks);
      const client = fakeClient({ getWorkingDiff });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("b.txt (New)"));

      expect(await screen.findByText(/staged content/)).toBeInTheDocument();
      expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "b.txt", true);
    });

    it("refetches the diff when status changes for the same selected file", async () => {
      let call = 0;
      const getWorkingDiff = vi.fn(async () => {
        call += 1;
        const content = call === 1 ? "before stage" : "after stage";
        return [
          {
            oldStart: 1,
            oldLines: 1,
            newStart: 1,
            newLines: 1,
            lines: [{ origin: "Context" as const, content }],
          },
        ];
      });
      const client = fakeClient({ getWorkingDiff });

      const { rerender } = render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("a.txt (Modified)"));
      expect(await screen.findByText(/before stage/)).toBeInTheDocument();

      // Simulate a stage/unstage/commit refresh: status is a new array (by reference), but
      // the same file (`a.txt`) is still selected — the pane must refetch, not keep showing
      // the pre-refresh diff.
      const refreshedStatus: StatusEntry[] = [
        { path: "a.txt", staged: true, kind: "Modified" },
        { path: "b.txt", staged: true, kind: "New" },
      ];
      rerender(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={refreshedStatus}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      await waitFor(() => expect(getWorkingDiff).toHaveBeenCalledTimes(2));
      expect(await screen.findByText(/after stage/)).toBeInTheDocument();
      expect(screen.queryByText(/before stage/)).not.toBeInTheDocument();
    });

    it("CommitBox is disabled when nothing is staged", () => {
      const client = fakeClient({});
      const unstagedOnly: StatusEntry[] = [{ path: "a.txt", staged: false, kind: "Modified" }];

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={unstagedOnly}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getByText("Commit")).toBeDisabled();
    });

    it("CommitBox is enabled when something is staged", () => {
      const client = fakeClient({});
      const stagedOnly: StatusEntry[] = [{ path: "b.txt", staged: true, kind: "New" }];

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={stagedOnly}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      // CommitBox itself still requires a non-empty message before it will enable
      // (covered by CommitBox.test.tsx); typing one here isolates DiffPane's own
      // concern, which is correctly threading disabled={stagedCount === 0}.
      fireEvent.change(screen.getByPlaceholderText("Commit message"), {
        target: { value: "a message" },
      });

      expect(screen.getByText("Commit")).not.toBeDisabled();
    });

    it("renders a Stash button", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getByText("Stash")).toBeInTheDocument();
    });

    it("clicking Stash calls onSaveStash", () => {
      const client = fakeClient({});
      const onSaveStash = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={onSaveStash}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("Stash"));

      expect(onSaveStash).toHaveBeenCalled();
    });

    it("Stash button is disabled when the working tree is clean", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getByText("Stash")).toBeDisabled();
    });

    it("Stash button is enabled when there are changes", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getByText("Stash")).not.toBeDisabled();
    });

    it("Stash button is disabled while a rebase is in progress", () => {
      // Stashing a paused rebase step's resolved/amended working tree away and then continuing
      // lands an empty or wrong commit, with nothing in the rebase state that notices — so the
      // action is off the table for the whole pause (see the backend's own HEAD-drift guard in
      // `git-core::rebase::rebase_continue`).
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={{ currentStep: 1, totalSteps: 3 }}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getByText("Stash")).toBeDisabled();
    });

    it("renders a Blame button per file", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getAllByText("Blame")).toHaveLength(2);
    });

    it("clicking Blame fetches and renders blame for that file", async () => {
      const blameLines: BlameLine[] = [
        {
          lineNumber: 1,
          content: "hello",
          commitId: "abc123",
          shortId: "abc1234",
          authorName: "Rene",
          timestamp: 1,
        },
      ];
      const getBlame = vi.fn(async () => blameLines);
      const client = fakeClient({ getBlame });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getAllByText("Blame")[0]);

      expect(await screen.findByText("hello")).toBeInTheDocument();
      expect(getBlame).toHaveBeenCalledWith(TEST_REPO_PATH, "HEAD", "a.txt");
    });

    it("clicking a blame line calls onSelectRow with that line's commit id", async () => {
      const blameLines: BlameLine[] = [
        {
          lineNumber: 1,
          content: "hello",
          commitId: "abc123",
          shortId: "abc1234",
          authorName: "Rene",
          timestamp: 1,
        },
      ];
      const getBlame = vi.fn(async () => blameLines);
      const client = fakeClient({ getBlame });
      const onSelectRow = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={onSelectRow}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getAllByText("Blame")[0]);
      const row = await screen.findByText("hello");
      fireEvent.click(row.closest("tr")!);

      expect(onSelectRow).toHaveBeenCalledWith({ commitId: "abc123" });
    });

    it("shows a friendly message, not the raw error, when blame fetch rejects", async () => {
      const getBlame = vi.fn(async () => {
        throw new Error("git operation failed: the path 'a.txt' does not exist in the given tree");
      });
      const client = fakeClient({ getBlame });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getAllByText("Blame")[0]);

      expect(
        await screen.findByText("No blame available for this file at this revision."),
      ).toBeInTheDocument();
      expect(screen.queryByText(/does not exist in the given tree/)).not.toBeInTheDocument();
    });

    it("Back to Diff switches back to the diff view", async () => {
      const blameLines: BlameLine[] = [
        {
          lineNumber: 1,
          content: "hello",
          commitId: "abc123",
          shortId: "abc1234",
          authorName: "Rene",
          timestamp: 1,
        },
      ];
      const getBlame = vi.fn(async () => blameLines);
      // Switching back to diff view re-triggers the diff-fetch effect (this file's diff was
      // never fetched, since we went straight to blame) — see UncommittedDiffPane's diff
      // effect comment on why every viewMode/selected change refetches.
      const getWorkingDiff = vi.fn(async () => []);
      const client = fakeClient({ getBlame, getWorkingDiff });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getAllByText("Blame")[0]);
      await screen.findByText("hello");
      fireEvent.click(screen.getByText("Back to Diff"));

      expect(screen.queryByText("Back to Diff")).not.toBeInTheDocument();
    });

    it("clicking a conflicted file opens the conflict resolution pane instead of a diff", async () => {
      const conflictedStatus: StatusEntry[] = [
        { path: "shared.txt", staged: false, kind: "Conflicted" },
      ];
      const client = fakeClient({
        getConflictHunks: async () => [{ kind: "Clean", content: "resolved already" }],
      });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={conflictedStatus}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("shared.txt (Conflicted)"));

      await waitFor(() => screen.getByText("Save resolution"));
    });

    it("closes the conflict pane when a status refresh no longer lists the selected path as Conflicted (e.g. after abort)", async () => {
      const conflictedStatus: StatusEntry[] = [
        { path: "shared.txt", staged: false, kind: "Conflicted" },
      ];
      const client = fakeClient({
        getConflictHunks: async () => [{ kind: "Clean", content: "resolved already" }],
        // Finding 5: once the conflicted entry disappears from `status`, `viewMode` now
        // transitions back to `"diff"`, which fires the normal diff-fetch effect for the
        // (no-longer-conflicted) selected file — so a real stub is needed here.
        getWorkingDiff: vi.fn(async () => []),
      });

      const { rerender } = render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={conflictedStatus}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={"Merge branch 'feature'"}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("shared.txt (Conflicted)"));
      await waitFor(() => screen.getByText("Save resolution"));

      // Simulate a status refresh after an abort (or after the conflict was already resolved
      // through this same pane): the conflicted entry is gone.
      rerender(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.queryByText("Save resolution")).not.toBeInTheDocument();
      // Finding 5: the pane falls back to the real diff view (which fetches), not a
      // permanently blank one — `viewMode` transitioned back to `"diff"`.
      await waitFor(() =>
        expect(client.getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "shared.txt", false),
      );
    });

    it("switching the selected conflicted file discards stale add/delete fallback state (remounts via key)", async () => {
      const twoConflicts: StatusEntry[] = [
        { path: "binary.dat", staged: false, kind: "Conflicted" },
        { path: "shared.txt", staged: false, kind: "Conflicted" },
      ];
      let resolveSharedHunks: (segments: ConflictSegment[]) => void = () => {};
      const getConflictHunks = vi.fn((_repoPath: string, path: string): Promise<ConflictSegment[]> => {
        if (path === "binary.dat") {
          return Promise.reject(
            new Error("'binary.dat' is an add/delete conflict, not a text conflict"),
          );
        }
        // shared.txt: a fetch that never resolves during this test, simulating the window
        // where the new path's fetch is still in flight.
        return new Promise((resolve) => {
          resolveSharedHunks = resolve;
        });
      });
      const client = fakeClient({ getConflictHunks });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={twoConflicts}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("binary.dat (Conflicted)"));
      await waitFor(() => screen.getByText("Keep Our Version"));

      fireEvent.click(screen.getByText("shared.txt (Conflicted)"));

      // The add/delete fallback buttons (bound to the OLD path) must not survive into the new
      // render while the new path's fetch is still pending — the `key={selected.path}` on
      // `ConflictResolutionPane` forces a fresh mount, discarding the stale state.
      expect(screen.queryByText("Keep Our Version")).not.toBeInTheDocument();
      expect(screen.queryByText("Keep Their Version")).not.toBeInTheDocument();
      expect(screen.queryByText("Delete File")).not.toBeInTheDocument();

      // Avoid an unresolved-promise/act warning leaking into other tests.
      resolveSharedHunks([]);
    });

    it("disables Commit while a Conflicted entry exists in status, even with staged files", () => {
      const mixedStatus: StatusEntry[] = [
        { path: "a.txt", staged: true, kind: "New" },
        { path: "shared.txt", staged: false, kind: "Conflicted" },
      ];
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={mixedStatus}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      const commitButton = screen.getByText("Commit").closest("button");
      expect(commitButton).toBeDisabled();
    });

    it("groups unstaged and staged entries under labelled headings with counts", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getByText("Changes (1)")).toBeInTheDocument();
      expect(screen.getByText("Staged (1)")).toBeInTheDocument();
    });

    it("marks the selected file's row as aria-selected", async () => {
      const client = fakeClient({ getWorkingDiff: async () => [] });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      const row = screen.getByText("a.txt (Modified)").closest('[role="option"]');
      expect(row).toHaveAttribute("aria-selected", "false");

      fireEvent.click(screen.getByText("a.txt (Modified)"));

      await waitFor(() => expect(row).toHaveAttribute("aria-selected", "true"));
    });

    it("renders a status-kind icon for each file row", () => {
      const client = fakeClient({});

      const { container } = render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      // One status icon per file row (2 entries in `status`). Scoped to the rows so an
      // unrelated icon elsewhere in the pane can't satisfy this.
      expect(container.querySelectorAll('li[role="option"] svg').length).toBeGreaterThanOrEqual(2);
    });

    it("navigates the unstaged group with ArrowDown/ArrowUp and updates the diff", async () => {
      const twoUnstaged: StatusEntry[] = [
        { path: "a.txt", staged: false, kind: "Modified" },
        { path: "c.txt", staged: false, kind: "New" },
      ];
      const getWorkingDiff = vi.fn(async (_repoPath: string, path: string) => [
        { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Context" as const, content: path }] },
      ]);
      const client = fakeClient({ getWorkingDiff });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={twoUnstaged}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("a.txt (Modified)"));
      await screen.findByText("a.txt");

      fireEvent.keyDown(screen.getByRole("listbox", { name: "Unstaged changes" }), { key: "ArrowDown" });

      expect(await screen.findByText("c.txt")).toBeInTheDocument();
    });

    // The per-row Stage/Unstage buttons live inside a `role="option"` row, where ARIA's
    // listbox pattern doesn't reliably expose them to assistive tech during arrow-key
    // navigation (see `primitives/ListRow.tsx`'s doc comment). `s` on the group container —
    // the listbox's single tab stop, which already owns `j`/`k`/arrow navigation — is the
    // keyboard-only path to the same action.
    it("stages the selected unstaged row when 's' is pressed on the group", async () => {
      const client = fakeClient({ getWorkingDiff: async () => [] });
      const onStageFile = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={onStageFile}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      const group = screen.getByRole("listbox", { name: "Unstaged changes" });
      // Select with the keyboard too, so nothing in this path depends on a pointer.
      fireEvent.keyDown(group, { key: "ArrowDown" });
      await waitFor(() =>
        expect(screen.getByText("a.txt (Modified)").closest('[role="option"]')).toHaveAttribute(
          "aria-selected",
          "true",
        ),
      );

      fireEvent.keyDown(group, { key: "s" });

      expect(onStageFile).toHaveBeenCalledWith("a.txt");
    });

    it("unstages the selected staged row when 's' is pressed on the staged group", async () => {
      const client = fakeClient({ getWorkingDiff: async () => [] });
      const onUnstageFile = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={onUnstageFile}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      const group = screen.getByRole("listbox", { name: "Staged changes" });
      fireEvent.keyDown(group, { key: "ArrowDown" });
      await waitFor(() =>
        expect(screen.getByText("b.txt (New)").closest('[role="option"]')).toHaveAttribute(
          "aria-selected",
          "true",
        ),
      );

      fireEvent.keyDown(group, { key: "s" });

      expect(onUnstageFile).toHaveBeenCalledWith("b.txt");
    });

    it("shows RebaseProgressPanel instead of CommitBox while a rebase is in progress", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={{ currentStep: 1, totalSteps: 3 }}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getByText(/Step 1 of 3/)).toBeInTheDocument();
      expect(screen.queryByPlaceholderText("Commit message")).not.toBeInTheDocument();
    });
  });

  describe("commit", () => {
    const getCommitFiles = vi.fn(async () => ["src/main.rs"]);
    const hunks: DiffHunk[] = [
      {
        oldStart: 1,
        oldLines: 1,
        newStart: 1,
        newLines: 1,
        lines: [{ origin: "Add", content: "fn main() {}" }],
      },
    ];
    const getCommitDiff = vi.fn(async () => hunks);

    it("renders the commit's changed files and their diff on click", async () => {
      const client = fakeClient({ getCommitFiles, getCommitDiff });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(await screen.findByText("src/main.rs")).toBeInTheDocument();

      fireEvent.click(screen.getByText("src/main.rs"));

      expect(await screen.findByText(/fn main/)).toBeInTheDocument();
      expect(getCommitDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "abc123", "src/main.rs");
    });

    it("no CommitBox or stage/unstage buttons render", async () => {
      const client = fakeClient({ getCommitFiles, getCommitDiff });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(await screen.findByText("src/main.rs")).toBeInTheDocument();

      expect(screen.queryByText("Commit")).toBeNull();
      expect(screen.queryByText("Stage")).toBeNull();
    });

    it("no Stash button renders for a commit's diff", async () => {
      const client = fakeClient({ getCommitFiles, getCommitDiff });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(await screen.findByText("src/main.rs")).toBeInTheDocument();

      expect(screen.queryByText("Stash")).toBeNull();
    });

    it("renders a Blame button per file", async () => {
      const client = fakeClient({ getCommitFiles, getCommitDiff });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(await screen.findByText("src/main.rs")).toBeInTheDocument();
      expect(screen.getByText("Blame")).toBeInTheDocument();
    });

    it("clicking Blame fetches and renders blame for that commit's file", async () => {
      const blameLines: BlameLine[] = [
        {
          lineNumber: 1,
          content: "fn main() {}",
          commitId: "abc123",
          shortId: "abc1234",
          authorName: "Rene",
          timestamp: 1,
        },
      ];
      const getBlame = vi.fn(async () => blameLines);
      const client = fakeClient({ getCommitFiles, getCommitDiff, getBlame });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      await screen.findByText("src/main.rs");
      fireEvent.click(screen.getByText("Blame"));

      expect(await screen.findByText("fn main() {}")).toBeInTheDocument();
      expect(getBlame).toHaveBeenCalledWith(TEST_REPO_PATH, "abc123", "src/main.rs");
    });

    it("clicking a blame line calls onSelectRow with that line's commit id", async () => {
      const blameLines: BlameLine[] = [
        {
          lineNumber: 1,
          content: "fn main() {}",
          commitId: "abc123",
          shortId: "abc1234",
          authorName: "Rene",
          timestamp: 1,
        },
      ];
      const getBlame = vi.fn(async () => blameLines);
      const client = fakeClient({ getCommitFiles, getCommitDiff, getBlame });
      const onSelectRow = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={onSelectRow}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      await screen.findByText("src/main.rs");
      fireEvent.click(screen.getByText("Blame"));
      const row = await screen.findByText("fn main() {}");
      fireEvent.click(row.closest("tr")!);

      expect(onSelectRow).toHaveBeenCalledWith({ commitId: "abc123" });
    });

    it("shows a friendly message, not the raw error, when blame fetch rejects", async () => {
      const getBlame = vi.fn(async () => {
        throw new Error(
          "git operation failed: the path 'src/main.rs' does not exist in the given tree",
        );
      });
      const client = fakeClient({ getCommitFiles, getCommitDiff, getBlame });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageAllFiles={vi.fn()}
          onUnstageAllFiles={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      await screen.findByText("src/main.rs");
      fireEvent.click(screen.getByText("Blame"));

      expect(
        await screen.findByText("No blame available for this file at this revision."),
      ).toBeInTheDocument();
      expect(screen.queryByText(/does not exist in the given tree/)).not.toBeInTheDocument();
    });
  });
});
