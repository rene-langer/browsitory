import { act, cleanup, fireEvent, render, render as rtlRender, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DEFAULT_DIFF_OPTIONS } from "../ipc/RepoClient";
import type { BlameLine, ConflictSegment, DiffHunk, RepoClient, StatusEntry } from "../ipc/RepoClient";
import { useState, type ReactNode } from "react";
import { DiffSettingsContext } from "../state/useDiffSettings";
import { CommitMessageSettingsContext } from "../state/useCommitMessageSettings";
import type { ExternalToolsApi } from "../state/useExternalTools";
import { DiffPane, type DiscardRequest } from "./DiffPane";
import styles from "./DiffPane.module.css";

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
    fetchAllRemotes: async () => unused(),
    pushBranch: async () => unused(),
    deleteRemoteBranch: async () => unused(),
    pullCurrentUpstream: async () => unused(),
    cancelTransfer: async () => unused(),
    subscribeTransferProgress: () => () => {},
    subscribeHookProgress: () => () => {},
    listStashes: unused,
    saveStash: unused,
    applyStash: unused,
    popStash: unused,
    getStashFiles: unused,
    getRangeFiles: unused,
    getRangeDiff: unused,
    getWorkingTreeCompareFiles: unused,
    getWorkingTreeCompareDiff: unused,
    getBranchCompareRange: unused,
    getMergeOptions: unused,
    setMergeOptions: unused,
    dropStash: unused,
    getBlame: unused,
    mergeBranch: unused,
    getConflictHunks: unused,
    resolveConflict: unused,
    abortMerge: unused,
    getMergeMessage: async () => null,
    getHeadInfo: async () => ({ headId: null, detachedAt: null, headPushed: false, operation: null, squashMessage: null, commitHooks: [], sequence: null }),
    amendCommit: async () => { throw new Error("unimplemented"); },
    getUndoState: async () => ({ undo: null, redo: null }),
    getCommitSignature: async () => ({ status: "unsigned", format: null, signer: null, key: null, detail: null }),
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
    openInEditor: async () => {},
    revealInFileManager: async () => {},
    openTerminal: async () => {},
    openDiffTool: async () => {},
    openMergeTool: async () => "Unresolved" as const,
    markResolved: async () => "Resolved" as const,
    resolveWholeFile: async () => {},
    cherryPickMany: async () => ({ kind: "Completed" as const, applied: 0, skipped: [] }),
    revertMany: async () => ({ kind: "Completed" as const, applied: 0, skipped: [] }),
    continueSequence: async () => ({ kind: "Completed" as const, applied: 0, skipped: [] }),
    abortSequence: async () => ({ rewound: true }),
    getExternalToolSettings: async () => ({ editor: null, terminal: null }),
    setExternalToolSettings: async () => {},
    getGitTools: async () => ({ diffTool: null, mergeTool: null }),
    getFileSides: async () => ({ old: { kind: "Missing" as const }, new: { kind: "Missing" as const } }),
    listRecentAuthors: async () => [],
    getCommitMessageSettings: async () => ({ summaryLimit: 50, bodyRuler: 72, showRuler: true }),
    setCommitMessageSettings: async () => {},
    getGraphBranchSelection: async () => null,
    setGraphBranchSelection: unused,
    getGraphRemoteBranchSelection: async () => null,
    setGraphRemoteBranchSelection: unused,
    getWorkingDiff: async () => [],
    getCommitDiff: async () => [],
    getFileHistory: unused,
    searchCommits: async () => ({ commits: [], nextCursor: null, scanned: 0 }),
    cancelSearch: async () => {},
    getCommitRenameDiff: unused,
    getCommitFiles: unused,
    getCommitMessage: unused,
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
    logFrontendError: unused,
    ...overrides,
  };
}

const noopHandlers = {
  onStageFile: vi.fn(),
  onUnstageFile: vi.fn(),
  onStageAllFiles: vi.fn(),
  onUnstageAllFiles: vi.fn(),
  onStageHunk: vi.fn(),
  onUnstageHunk: vi.fn(),
  onDiscardHunk: vi.fn(),
  onStageLines: vi.fn(),
  onUnstageLines: vi.fn(),
  onDiscardLines: vi.fn(),
  onCommit: vi.fn(async () => true),
  onSaveStash: vi.fn(),
  onOpenStashDialog: vi.fn(),
  onSelectRow: vi.fn(),
  onResolveConflict: vi.fn(),
  onResolveAddDeleteConflict: vi.fn(),
  onDiscardFile: vi.fn(),
  onDiscardAllUnstaged: vi.fn(),
  onIgnorePath: vi.fn(),
  onAbortMerge: vi.fn(),
  onRebaseContinue: vi.fn(),
  onRebaseAbort: vi.fn(),
};

function renderUncommitted(
  client: RepoClient,
  status: StatusEntry[],
  overrides: Partial<React.ComponentProps<typeof DiffPane>> = {},
) {
  return render(
    <DiffPane
      repoPath={TEST_REPO_PATH}
      client={client}
      selectedRow="uncommitted"
      status={status}
      mergeMessage={null}
      rebaseProgress={null}
      {...noopHandlers}
      {...overrides}
    />,
  );
}

function renderCommit(client: RepoClient, commitId: string, overrides: Partial<React.ComponentProps<typeof DiffPane>> = {}) {
  return render(
    <DiffPane
      repoPath={TEST_REPO_PATH}
      client={client}
      selectedRow={{ commitId }}
      status={[]}
      mergeMessage={null}
      rebaseProgress={null}
      {...noopHandlers}
      {...overrides}
    />,
  );
}

describe("DiffPane", () => {
  describe("uncommitted", () => {
    const status: StatusEntry[] = [
      { path: "a.txt", staged: false, kind: "Modified" },
      { path: "b.txt", staged: true, kind: "New" },
    ];

    it("focuses the commit message once the amend prefill is in and reports it", async () => {
      const onCommitMessageFocused = vi.fn();
      renderUncommitted(fakeClient({ getCommitMessage: async () => "head message" }), status, {
        amendMode: true,
        headInfo: { headId: "abc", detachedAt: null, headPushed: false, operation: null, squashMessage: null, commitHooks: [], sequence: null },
        focusCommitMessage: true,
        onCommitMessageFocused,
      });

      await waitFor(() => expect(screen.getByLabelText("Commit message")).toHaveValue("head message"));
      // Focus runs in a passive effect after the prefill commit, so under load it can land a tick
      // after the value is visible.
      await waitFor(() => expect(screen.getByLabelText("Commit message")).toHaveFocus());
      expect(onCommitMessageFocused).toHaveBeenCalled();
    });

    it("renders an icon-only Stage control for unstaged entries and Unstage for staged ones", () => {
      renderUncommitted(fakeClient({}), status);

      expect(screen.getByRole("button", { name: "Stage a.txt" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Unstage b.txt" })).toBeInTheDocument();
    });

    it("renders every file's diff expanded by default, with no click needed", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "changed line" }] },
      ];
      const getWorkingDiff = vi.fn(async () => hunks);
      renderUncommitted(fakeClient({ getWorkingDiff }), status);

      expect(await screen.findAllByText(/changed line/)).toHaveLength(2);
      expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "a.txt", false, DEFAULT_DIFF_OPTIONS);
      expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "b.txt", true, DEFAULT_DIFF_OPTIONS);
    });

    it("shows Stage Hunk (not Unstage Hunk) and Discard Hunk for an unstaged file's diff", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      // Only a.txt (unstaged) gets hunks — b.txt (staged) stays empty, so the assertions below
      // are unambiguous now that both sections render at once.
      const getWorkingDiff = vi.fn(async (_repoPath: string, path: string) => (path === "a.txt" ? hunks : []));
      renderUncommitted(fakeClient({ getWorkingDiff }), status);

      expect(await screen.findByText("Stage hunk")).toBeInTheDocument();
      expect(screen.queryByText("Unstage hunk")).not.toBeInTheDocument();
      expect(screen.getByText("Discard hunk")).toBeInTheDocument();
    });

    it("shows Unstage Hunk (not Stage Hunk) for a staged file's diff", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      // Only b.txt (staged) gets hunks this time — a.txt stays empty.
      const getWorkingDiff = vi.fn(async (_repoPath: string, path: string) => (path === "b.txt" ? hunks : []));
      renderUncommitted(fakeClient({ getWorkingDiff }), status);

      expect(await screen.findByText("Unstage hunk")).toBeInTheDocument();
      expect(screen.queryByText("Stage hunk")).not.toBeInTheDocument();
    });

    it("clicking Stage Hunk calls onStageHunk with that section's path and the hunk's start lines", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 3, oldLines: 1, newStart: 4, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      const onStageHunk = vi.fn();
      renderUncommitted(fakeClient({ getWorkingDiff: async () => hunks }), status, { onStageHunk });

      fireEvent.click(await screen.findByText("Stage hunk"));

      expect(onStageHunk).toHaveBeenCalledWith("a.txt", 3, 4, 3);
    });

    it("refetches only the file whose hunk was staged, not every other open section (PERF-001)", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 3, oldLines: 1, newStart: 4, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      // Only a.txt (unstaged) gets a hunk — b.txt stays empty, so there's exactly one "Stage
      // hunk" button to click.
      const getWorkingDiff = vi.fn(async (_repoPath: string, path: string) => (path === "a.txt" ? hunks : []));
      const client = fakeClient({ getWorkingDiff });
      const { rerender } = renderUncommitted(client, status);

      await waitFor(() => expect(getWorkingDiff).toHaveBeenCalledTimes(2));
      getWorkingDiff.mockClear();

      fireEvent.click(await screen.findByText("Stage hunk"));

      // A status array that is new by reference but didn't come from a refresh (same content,
      // different identity, and no `refreshGeneration` change) must not refetch every open
      // section on its own — only the path the hunk action touched is invalidated. (A real
      // refresh *does* invalidate every open section; see the `refreshGeneration` test below.)
      rerender(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={[...status]}
          mergeMessage={null}
          rebaseProgress={null}
          {...noopHandlers}
        />,
      );

      await waitFor(() => expect(getWorkingDiff).toHaveBeenCalledTimes(1));
      expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "a.txt", false, DEFAULT_DIFF_OPTIONS);
      expect(getWorkingDiff).not.toHaveBeenCalledWith(TEST_REPO_PATH, "b.txt", true, DEFAULT_DIFF_OPTIONS);
    });

    it("refetches both sides of a partially-staged file, only after the hunk action settles", async () => {
      const partial: StatusEntry[] = [
        { path: "a.txt", staged: false, kind: "Modified" },
        { path: "a.txt", staged: true, kind: "Modified" },
      ];
      const hunk: DiffHunk = { oldStart: 3, oldLines: 1, newStart: 4, newLines: 1, lines: [{ origin: "Add", content: "x" }] };
      // Only the unstaged side has a hunk to stage, so there's exactly one "Stage hunk" button.
      const getWorkingDiff = vi.fn(async (_repoPath: string, _path: string, staged: boolean) => (staged ? [] : [hunk]));
      let settle: () => void = () => {};
      const onStageHunk = vi.fn(
        () =>
          new Promise<void>((resolve) => {
            settle = resolve;
          }),
      );
      renderUncommitted(fakeClient({ getWorkingDiff }), partial, { onStageHunk });

      await waitFor(() => expect(getWorkingDiff).toHaveBeenCalledTimes(2));
      getWorkingDiff.mockClear();

      fireEvent.click(await screen.findByText("Stage hunk"));
      expect(onStageHunk).toHaveBeenCalledWith("a.txt", 3, 4, 3);
      // Nothing refetches while the mutation is still in flight — a refetch now could read the
      // index before the backend applied the change, with nothing to correct it afterward.
      await act(async () => {});
      expect(getWorkingDiff).not.toHaveBeenCalled();

      await act(async () => settle());

      await waitFor(() => expect(getWorkingDiff).toHaveBeenCalledTimes(2));
      expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "a.txt", false, DEFAULT_DIFF_OPTIONS);
      expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "a.txt", true, DEFAULT_DIFF_OPTIONS);
    });

    it("refetches every open diff when refreshGeneration advances, with no hunk action involved", async () => {
      const getWorkingDiff = vi.fn(async () => []);
      const client = fakeClient({ getWorkingDiff });
      const { rerender } = renderUncommitted(client, status, { refreshGeneration: 0 });

      await waitFor(() => expect(getWorkingDiff).toHaveBeenCalledTimes(2));
      getWorkingDiff.mockClear();

      // What a palette Refresh, stash apply or pull looks like from here: `useAppState.refresh()`
      // advances `refreshGeneration`, while `status` may well be content-identical.
      rerender(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          refreshGeneration={1}
          mergeMessage={null}
          rebaseProgress={null}
          {...noopHandlers}
        />,
      );

      await waitFor(() => expect(getWorkingDiff).toHaveBeenCalledTimes(2));
      expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "a.txt", false, DEFAULT_DIFF_OPTIONS);
      expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "b.txt", true, DEFAULT_DIFF_OPTIONS);
    });

    it("clicking the Stage control calls onStageFile with that path", () => {
      const onStageFile = vi.fn();
      renderUncommitted(fakeClient({}), status, { onStageFile });

      fireEvent.click(screen.getByRole("button", { name: "Stage a.txt" }));

      expect(onStageFile).toHaveBeenCalledWith("a.txt");
    });

    describe("file actions", () => {
      const fileStatus: StatusEntry[] = [
        { path: "src/a.ts", staged: false, kind: "Modified" },
        { path: "logs/run.log", staged: false, kind: "New" },
        { path: "shared.txt", staged: false, kind: "Conflicted" },
      ];
      const render = (overrides: Partial<React.ComponentProps<typeof DiffPane>> = {}) =>
        renderUncommitted(fakeClient({ getConflictHunks: async () => [] }), fileStatus, overrides);

      it("discards a tracked file after confirming from the actions button", () => {
        const onDiscardFile = vi.fn();
        render({ onDiscardFile });

        fireEvent.click(screen.getByRole("button", { name: "Actions for src/a.ts" }));
        fireEvent.click(screen.getByRole("menuitem", { name: "Discard changes…" }));
        const dialog = screen.getByRole("dialog", { name: "Discard changes", hidden: true });
        expect(within(dialog).getByText("Discard changes to src/a.ts? This can't be undone.")).toBeInTheDocument();
        expect(onDiscardFile).not.toHaveBeenCalled();
        fireEvent.click(within(dialog).getByText("Discard changes"));

        expect(onDiscardFile).toHaveBeenCalledWith("src/a.ts");
      });

      it("does not discard when the dialog is cancelled", () => {
        const onDiscardFile = vi.fn();
        render({ onDiscardFile });

        fireEvent.click(screen.getByRole("button", { name: "Actions for src/a.ts" }));
        fireEvent.click(screen.getByRole("menuitem", { name: "Discard changes…" }));
        fireEvent.click(screen.getByText("Cancel"));

        expect(onDiscardFile).not.toHaveBeenCalled();
      });

      it("opens the same menu on right-click of the file header", () => {
        render();

        fireEvent.contextMenu(screen.getByText("src/a.ts (Modified)"));

        expect(screen.getByRole("menuitem", { name: "Discard changes…" })).toBeInTheDocument();
      });

      it("opens the menu for the selected row with Shift+F10", () => {
        render();
        fireEvent.click(screen.getByText("src/a.ts (Modified)"));

        fireEvent.keyDown(screen.getByRole("listbox", { name: "Unstaged changes" }), { key: "F10", shiftKey: true });

        expect(screen.getByRole("menuitem", { name: "Discard changes…" })).toBeInTheDocument();
      });

      it("keeps list key bindings out of an open menu", () => {
        const onStageFile = vi.fn();
        const onUnstageFile = vi.fn();
        render({ onStageFile, onUnstageFile });
        fireEvent.click(screen.getByText("src/a.ts (Modified)"));
        fireEvent.click(screen.getByRole("button", { name: "Actions for src/a.ts" }));
        const item = screen.getByRole("menuitem", { name: "Discard changes…" });

        fireEvent.keyDown(item, { key: "ArrowDown" });
        fireEvent.keyDown(item, { key: "s" });

        expect(screen.getByRole("option", { name: /src\/a\.ts/, selected: true })).toBeInTheDocument();
        expect(screen.getByRole("menuitem", { name: "Discard changes…" })).toBeInTheDocument();
        expect(onStageFile).not.toHaveBeenCalled();
        expect(onUnstageFile).not.toHaveBeenCalled();
      });

      it("keeps list key bindings out of the discard dialog", () => {
        const onStageFile = vi.fn();
        const onUnstageFile = vi.fn();
        render({ onStageFile, onUnstageFile });
        fireEvent.click(screen.getByRole("button", { name: "Actions for src/a.ts" }));
        fireEvent.click(screen.getByRole("menuitem", { name: "Discard changes…" }));

        fireEvent.keyDown(screen.getByText("Cancel"), { key: "s" });

        expect(onStageFile).not.toHaveBeenCalled();
        expect(onUnstageFile).not.toHaveBeenCalled();
      });

      it("opens the file menu only from the header, not the diff body", async () => {
        const hunks: DiffHunk[] = [
          { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "body line" }] },
        ];
        renderUncommitted(fakeClient({ getWorkingDiff: async () => hunks }), [fileStatus[0]]);

        fireEvent.contextMenu(await screen.findByText(/body line/));
        expect(screen.queryByRole("menuitem")).not.toBeInTheDocument();

        fireEvent.contextMenu(screen.getByText("src/a.ts (Modified)"));
        expect(screen.getByRole("menuitem", { name: "Discard changes…" })).toBeInTheDocument();
      });

      it("words the untracked delete dialog and ignores without confirming", () => {
        const onIgnorePath = vi.fn();
        const onDiscardFile = vi.fn();
        render({ onIgnorePath, onDiscardFile });

        fireEvent.click(screen.getByRole("button", { name: "Actions for logs/run.log" }));
        fireEvent.click(screen.getByRole("menuitem", { name: "Ignore all *.log" }));

        expect(onIgnorePath).toHaveBeenCalledWith("*.log");
        expect(screen.queryByRole("dialog", { hidden: true })).not.toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "Actions for logs/run.log" }));
        fireEvent.click(screen.getByRole("menuitem", { name: "Discard changes…" }));
        expect(screen.getByText("Delete untracked file logs/run.log? This can't be undone.")).toBeInTheDocument();
        expect(screen.getByText("Delete file")).toBeInTheDocument();
      });

      it("stashes an unstaged file from its actions menu", () => {
        const onOpenStashDialog = vi.fn();
        render({ onOpenStashDialog });

        fireEvent.click(screen.getByRole("button", { name: "Actions for src/a.ts" }));
        fireEvent.click(screen.getByRole("menuitem", { name: "Stash file…" }));

        expect(onOpenStashDialog).toHaveBeenCalledWith(["src/a.ts"]);
      });

      it("gives conflicted files no actions button", () => {
        render();

        expect(screen.queryByRole("button", { name: "Actions for shared.txt" })).not.toBeInTheDocument();
      });

      it("Discard all confirms then calls onDiscardAllUnstaged", () => {
        const onDiscardAllUnstaged = vi.fn();
        render({ onDiscardAllUnstaged });

        fireEvent.click(screen.getByRole("button", { name: "Discard all…" }));
        expect(
          screen.getByText("Discard changes to 2 files (1 untracked will be deleted)? This can't be undone."),
        ).toBeInTheDocument();
        expect(onDiscardAllUnstaged).not.toHaveBeenCalled();
        fireEvent.click(screen.getByText("Discard all"));

        expect(onDiscardAllUnstaged).toHaveBeenCalledTimes(1);
        // Exactly what the dialog counted: the unstaged, non-conflicted entries.
        expect(onDiscardAllUnstaged).toHaveBeenCalledWith(["src/a.ts", "logs/run.log"]);
      });

      it("opens the file confirm from a lifted request and reports the current file", () => {
        const onDiscardFile = vi.fn();
        const onDiscardRequestChange = vi.fn();
        const onCurrentFileChange = vi.fn();
        render({
          onDiscardFile,
          discardRequest: { kind: "file", path: "src/a.ts" },
          onDiscardRequestChange,
          onCurrentFileChange,
        });

        fireEvent.click(screen.getByText("Discard changes"));

        expect(onDiscardFile).toHaveBeenCalledWith("src/a.ts");
        expect(onDiscardRequestChange).toHaveBeenCalledWith(null);
        expect(onCurrentFileChange).toHaveBeenCalledWith(null);
      });

      it("clears a file request whose path disappears, and does not reopen when it returns", () => {
        function Harness({ status }: { status: StatusEntry[] }) {
          const [req, setReq] = useState<DiscardRequest | null>({ kind: "file", path: "src/a.ts" });
          return (
            <DiffPane
              repoPath={TEST_REPO_PATH}
              client={fakeClient({ getConflictHunks: async () => [] })}
              selectedRow="uncommitted"
              status={status}
              mergeMessage={null}
              rebaseProgress={null}
              {...noopHandlers}
              discardRequest={req}
              onDiscardRequestChange={setReq}
            />
          );
        }
        const { rerender } = rtlRender(<Harness status={fileStatus} />);
        expect(screen.getByText("Discard changes")).toBeInTheDocument();

        rerender(<Harness status={[fileStatus[1]]} />);
        expect(screen.queryByText("Discard changes")).not.toBeInTheDocument();

        rerender(<Harness status={fileStatus} />);
        expect(screen.queryByText("Discard changes")).not.toBeInTheDocument();
      });

      it("clears an all request when nothing is discardable and shows no dialog", () => {
        const onDiscardRequestChange = vi.fn();
        renderUncommitted(fakeClient({}), [], { discardRequest: { kind: "all" }, onDiscardRequestChange });

        expect(onDiscardRequestChange).toHaveBeenCalledWith(null);
        expect(screen.queryByRole("button", { name: "Discard all" })).not.toBeInTheDocument();
      });

      it("disables Discard all when every unstaged entry is conflicted", () => {
        renderUncommitted(fakeClient({ getConflictHunks: async () => [] }), [fileStatus[2]]);

        expect(screen.getByRole("button", { name: "Discard all…" })).toBeDisabled();
      });
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
      const onStageFile = vi.fn();
      const onStageAllFiles = vi.fn();
      renderUncommitted(fakeClient({}), threeStatus, { onStageFile, onStageAllFiles });

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
      const onStageAllFiles = vi.fn();
      renderUncommitted(fakeClient({ getConflictHunks: async () => [] }), conflictedMix, { onStageAllFiles });

      fireEvent.click(screen.getByRole("button", { name: "Stage all" }));

      expect(onStageAllFiles).toHaveBeenCalledWith(["a.txt"]);
      // The conflicted file keeps its own per-row Stage control — only the bulk action skips it.
      expect(screen.getByRole("button", { name: "Stage shared.txt" })).toBeInTheDocument();
    });

    it("disables Stage all when every unstaged entry is Conflicted", () => {
      const conflictsOnly: StatusEntry[] = [{ path: "shared.txt", staged: false, kind: "Conflicted" }];
      renderUncommitted(fakeClient({ getConflictHunks: async () => [] }), conflictsOnly);

      expect(screen.getByRole("button", { name: "Stage all" })).toBeDisabled();
    });

    it("Unstage all makes a single bulk call with every staged path", () => {
      const onUnstageFile = vi.fn();
      const onUnstageAllFiles = vi.fn();
      renderUncommitted(fakeClient({}), status, { onUnstageFile, onUnstageAllFiles });

      fireEvent.click(screen.getByRole("button", { name: "Unstage all" }));

      expect(onUnstageAllFiles).toHaveBeenCalledWith(["b.txt"]);
      expect(onUnstageAllFiles).toHaveBeenCalledTimes(1);
      expect(onUnstageFile).not.toHaveBeenCalled();
    });

    it("does not render Stage all when there are no unstaged entries", () => {
      const stagedOnly: StatusEntry[] = [{ path: "b.txt", staged: true, kind: "New" }];
      renderUncommitted(fakeClient({}), stagedOnly);

      expect(screen.queryByRole("button", { name: "Stage all" })).not.toBeInTheDocument();
    });

    it("CommitBox is disabled when nothing is staged", () => {
      const unstagedOnly: StatusEntry[] = [{ path: "a.txt", staged: false, kind: "Modified" }];
      renderUncommitted(fakeClient({}), unstagedOnly);

      expect(screen.getByText("Commit")).toBeDisabled();
    });

    it("CommitBox is enabled when something is staged", () => {
      const stagedOnly: StatusEntry[] = [{ path: "b.txt", staged: true, kind: "New" }];
      renderUncommitted(fakeClient({}), stagedOnly);

      // CommitBox itself still requires a non-empty message before it will enable
      // (covered by CommitBox.test.tsx); typing one here isolates DiffPane's own
      // concern, which is correctly threading disabled={stagedCount === 0}.
      fireEvent.change(screen.getByPlaceholderText("Commit message"), {
        target: { value: "a message" },
      });

      expect(screen.getByText("Commit")).not.toBeDisabled();
    });

    it("renders a Stash button", () => {
      renderUncommitted(fakeClient({}), status);

      expect(screen.getByText("Stash")).toBeInTheDocument();
    });

    it("clicking Stash calls onSaveStash", () => {
      const onSaveStash = vi.fn();
      renderUncommitted(fakeClient({}), status, { onSaveStash });

      fireEvent.click(screen.getByText("Stash"));

      expect(onSaveStash).toHaveBeenCalled();
    });

    it("Stash button is disabled when the working tree is clean", () => {
      renderUncommitted(fakeClient({}), []);

      expect(screen.getByText("Stash")).toBeDisabled();
    });

    it("Stash button is enabled when there are changes", () => {
      renderUncommitted(fakeClient({}), status);

      expect(screen.getByText("Stash")).not.toBeDisabled();
    });

    it("Stash button is disabled while a rebase is in progress", () => {
      // Stashing a paused rebase step's resolved/amended working tree away and then continuing
      // lands an empty or wrong commit, with nothing in the rebase state that notices — so the
      // action is off the table for the whole pause (see the backend's own HEAD-drift guard in
      // `git-core::rebase::rebase_continue`).
      renderUncommitted(fakeClient({}), status, { rebaseProgress: { currentStep: 1, totalSteps: 3 } });

      expect(screen.getByText("Stash")).toBeDisabled();
    });

    it("opens the stash dialog or stashes all from the Stash chevron menu", () => {
      const onSaveStash = vi.fn();
      const onOpenStashDialog = vi.fn();
      renderUncommitted(fakeClient({}), status, { onSaveStash, onOpenStashDialog });

      fireEvent.click(screen.getByRole("button", { name: "More stash options" }));
      fireEvent.click(screen.getByRole("menuitem", { name: "Stash…" }));
      expect(onOpenStashDialog).toHaveBeenCalledWith([]);

      fireEvent.click(screen.getByRole("button", { name: "More stash options" }));
      fireEvent.click(screen.getByRole("menuitem", { name: "Stash all" }));
      expect(onSaveStash).toHaveBeenCalled();
    });

    it("disables the Stash chevron with the main button", () => {
      renderUncommitted(fakeClient({}), []);

      expect(screen.getByRole("button", { name: "More stash options" })).toBeDisabled();
    });

    it("offers Stash file… on a staged row's actions menu", () => {
      const onOpenStashDialog = vi.fn();
      renderUncommitted(fakeClient({}), [{ path: "src/s.ts", staged: true, kind: "Modified" }], {
        onOpenStashDialog,
      });

      fireEvent.click(screen.getByRole("button", { name: "Actions for src/s.ts" }));
      expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Stash file…"]);
      fireEvent.click(screen.getByRole("menuitem", { name: "Stash file…" }));

      expect(onOpenStashDialog).toHaveBeenCalledWith(["src/s.ts"]);
    });

    it("pre-fills a pending squash message without offering Abort merge", () => {
      renderUncommitted(fakeClient({}), status, {
        mergeMessage: null,
        headInfo: {
          headId: "abc",
          detachedAt: null,
          headPushed: false,
          operation: null,
          squashMessage: "Squashed commit of the following:\n",
          commitHooks: [], sequence: null,
        },
      });

      expect(screen.getByPlaceholderText("Commit message")).toHaveValue("Squashed commit of the following:\n");
      expect(screen.queryByRole("button", { name: "Abort merge" })).toBeNull();
    });

    it("keeps the rebase actions in a pinned commit-actions group so they never scroll away with long diffs", () => {
      renderUncommitted(fakeClient({}), status, { rebaseProgress: { currentStep: 1, totalSteps: 3 } });

      const dock = screen.getByRole("group", { name: "Commit actions" });
      expect(within(dock).getByText("Stash")).toBeInTheDocument();
      expect(within(dock).getByRole("button", { name: /abort rebase/i })).toBeInTheDocument();
    });

    it("shows a paused cherry-pick sequence in the dock instead of the commit box", () => {
      const onSequenceContinue = vi.fn();
      const onSequenceAbort = vi.fn();
      const sequence = { kind: "CherryPick" as const, currentStep: 2, totalSteps: 5, currentCommit: "c" };
      const headInfo = { headId: "abc", detachedAt: null, headPushed: false, operation: "CherryPick" as const, squashMessage: null, commitHooks: [], sequence };
      const { unmount } = renderUncommitted(
        fakeClient({ getConflictHunks: async () => [] }),
        [...status, { path: "c.txt", staged: false, kind: "Conflicted" }],
        { headInfo, onSequenceContinue, onSequenceAbort },
      );

      const dock = screen.getByRole("group", { name: "Commit actions" });
      expect(within(dock).getByText("Cherry-pick in progress")).toBeInTheDocument();
      expect(within(dock).getByText("Step 2 of 5")).toBeInTheDocument();
      expect(within(dock).getByRole("button", { name: "Continue cherry-pick" })).toBeDisabled();
      expect(within(dock).queryByRole("textbox", { name: "Commit message" })).toBeNull();
      expect(within(dock).getByRole("button", { name: "Stash" })).toBeDisabled();
      fireEvent.click(within(dock).getByRole("button", { name: "Abort cherry-pick" }));
      expect(onSequenceAbort).toHaveBeenCalled();
      unmount();

      // Stopped by an error: nothing conflicted, so Continue (a retry) is enabled.
      renderUncommitted(fakeClient({}), status, {
        headInfo: { ...headInfo, operation: null, sequence: { ...sequence, kind: "Revert" } },
        onSequenceContinue,
        onSequenceAbort,
      });
      const revertDock = screen.getByRole("group", { name: "Commit actions" });
      expect(within(revertDock).getByText("Revert in progress")).toBeInTheDocument();
      fireEvent.click(within(revertDock).getByRole("button", { name: "Continue revert" }));
      expect(onSequenceContinue).toHaveBeenCalled();
    });

    it("disables the sequence's Continue and Abort while an action is pending", () => {
      const sequence = { kind: "Revert" as const, currentStep: 1, totalSteps: 2, currentCommit: "c" };
      renderUncommitted(fakeClient({}), status, {
        headInfo: { headId: "abc", detachedAt: null, headPushed: false, operation: null, squashMessage: null, commitHooks: [], sequence },
        sequencePending: true,
      });
      const dock = screen.getByRole("group", { name: "Commit actions" });
      expect(within(dock).getByRole("button", { name: "Continue revert" })).toBeDisabled();
      expect(within(dock).getByRole("button", { name: "Abort revert" })).toBeDisabled();
    });

    it("keeps the commit box in the pinned commit-actions group", () => {
      renderUncommitted(fakeClient({}), status);

      const dock = screen.getByRole("group", { name: "Commit actions" });
      expect(within(dock).getByRole("textbox", { name: "Commit message" })).toBeInTheDocument();
    });

    it("renders a Blame button per file", () => {
      renderUncommitted(fakeClient({}), status);

      expect(screen.getAllByText("Blame")).toHaveLength(2);
    });

    it("clicking Blame fetches and renders blame for that file", async () => {
      const blameLines: BlameLine[] = [
        { lineNumber: 1, content: "hello", commitId: "abc123", shortId: "abc1234", authorName: "Rene", timestamp: 1 },
      ];
      const getBlame = vi.fn(async () => blameLines);
      renderUncommitted(fakeClient({ getBlame, getWorkingDiff: async () => [] }), status);

      fireEvent.click(screen.getAllByText("Blame")[0]);

      expect(await screen.findByText("hello")).toBeInTheDocument();
      expect(getBlame).toHaveBeenCalledWith(TEST_REPO_PATH, "HEAD", "a.txt");
    });

    it("clicking a blame line calls onSelectRow with that line's commit id", async () => {
      const blameLines: BlameLine[] = [
        { lineNumber: 1, content: "hello", commitId: "abc123", shortId: "abc1234", authorName: "Rene", timestamp: 1 },
      ];
      const getBlame = vi.fn(async () => blameLines);
      const onSelectRow = vi.fn();
      renderUncommitted(fakeClient({ getBlame, getWorkingDiff: async () => [] }), status, { onSelectRow });

      fireEvent.click(screen.getAllByText("Blame")[0]);
      const row = await screen.findByText("hello");
      fireEvent.click(row.closest("tr")!);

      expect(onSelectRow).toHaveBeenCalledWith({ commitId: "abc123" });
    });

    it("shows a friendly message, not the raw error, when blame fetch rejects", async () => {
      const getBlame = vi.fn(async () => {
        throw new Error("git operation failed: the path 'a.txt' does not exist in the given tree");
      });
      renderUncommitted(fakeClient({ getBlame, getWorkingDiff: async () => [] }), status);

      fireEvent.click(screen.getAllByText("Blame")[0]);

      expect(await screen.findByText("No blame available for this file at this revision.")).toBeInTheDocument();
      expect(screen.queryByText(/does not exist in the given tree/)).not.toBeInTheDocument();
    });

    it("shows the raw error and a plain-language hint, plus a Retry that re-fetches, when the working diff fails", async () => {
      const singleFile: StatusEntry[] = [{ path: "a.txt", staged: false, kind: "Modified" }];
      const getWorkingDiff = vi.fn<RepoClient["getWorkingDiff"]>().mockResolvedValue([]);
      getWorkingDiff.mockRejectedValueOnce(new Error("Transport failed: sidecar exited unexpectedly (code 1)"));
      renderUncommitted(fakeClient({ getWorkingDiff }), singleFile);

      expect(
        await screen.findByText("Transport failed: sidecar exited unexpectedly (code 1)"),
      ).toBeInTheDocument();
      expect(
        screen.getByText("The connection to the backend was lost. Retry, or reopen the repository."),
      ).toBeInTheDocument();
      expect(getWorkingDiff).toHaveBeenCalledTimes(1);

      fireEvent.click(screen.getByRole("button", { name: "Retry" }));

      await waitFor(() => expect(getWorkingDiff).toHaveBeenCalledTimes(2));
      expect(
        screen.queryByText("Transport failed: sidecar exited unexpectedly (code 1)"),
      ).not.toBeInTheDocument();
    });

    it("shows a Retry that re-fetches blame when the blame fetch fails", async () => {
      const getBlame = vi
        .fn<RepoClient["getBlame"]>()
        .mockRejectedValueOnce(new Error("git operation failed: no such revision"))
        .mockResolvedValueOnce([]);
      renderUncommitted(fakeClient({ getBlame, getWorkingDiff: async () => [] }), status);

      fireEvent.click(screen.getAllByText("Blame")[0]);
      await screen.findByText("No blame available for this file at this revision.");
      expect(getBlame).toHaveBeenCalledTimes(1);

      fireEvent.click(screen.getByRole("button", { name: "Retry" }));

      await waitFor(() => expect(getBlame).toHaveBeenCalledTimes(2));
      expect(screen.queryByText("No blame available for this file at this revision.")).not.toBeInTheDocument();
    });

    it("Back to diff switches that section back to the diff view", async () => {
      const blameLines: BlameLine[] = [
        { lineNumber: 1, content: "hello", commitId: "abc123", shortId: "abc1234", authorName: "Rene", timestamp: 1 },
      ];
      const getBlame = vi.fn(async () => blameLines);
      const getWorkingDiff = vi.fn(async () => []);
      renderUncommitted(fakeClient({ getBlame, getWorkingDiff }), status);

      fireEvent.click(screen.getAllByText("Blame")[0]);
      await screen.findByText("hello");
      fireEvent.click(screen.getByText("Back to diff"));

      expect(screen.queryByText("Back to diff")).not.toBeInTheDocument();
      expect(screen.queryByText("hello")).not.toBeInTheDocument();
    });

    it("shows the conflict resolution pane instead of a diff for a Conflicted file", async () => {
      const conflictedStatus: StatusEntry[] = [{ path: "shared.txt", staged: false, kind: "Conflicted" }];
      const client = fakeClient({ getConflictHunks: async () => [{ kind: "Clean", content: "resolved already" }] });
      renderUncommitted(client, conflictedStatus);

      await waitFor(() => screen.getByText("Save resolution"));
    });

    it("falls back to the real diff (fetched fresh) once a status refresh no longer lists the path as Conflicted", async () => {
      const conflictedStatus: StatusEntry[] = [{ path: "shared.txt", staged: false, kind: "Conflicted" }];
      const client = fakeClient({
        getConflictHunks: async () => [{ kind: "Clean", content: "resolved already" }],
        getWorkingDiff: vi.fn(async () => []),
      });

      const { rerender } = renderUncommitted(client, conflictedStatus, { mergeMessage: "Merge branch 'feature'" });
      await waitFor(() => screen.getByText("Save resolution"));

      // Simulate a status refresh after an abort (or after the conflict was already resolved
      // through this same pane): the conflicted entry is gone, replaced by its resolved form.
      const resolvedStatus: StatusEntry[] = [{ path: "shared.txt", staged: false, kind: "Modified" }];
      rerender(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={resolvedStatus}
          {...noopHandlers}
          mergeMessage={null}
          rebaseProgress={null}
        />,
      );

      expect(screen.queryByText("Save resolution")).not.toBeInTheDocument();
      await waitFor(() => expect(client.getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "shared.txt", false, DEFAULT_DIFF_OPTIONS));
    });

    it("keeps each conflicted file's resolution state independent of the others", async () => {
      const twoConflicts: StatusEntry[] = [
        { path: "binary.dat", staged: false, kind: "Conflicted" },
        { path: "shared.txt", staged: false, kind: "Conflicted" },
      ];
      const getConflictHunks = vi.fn((_repoPath: string, path: string): Promise<ConflictSegment[]> => {
        if (path === "binary.dat") {
          return Promise.reject(new Error("'binary.dat' is an add/delete conflict, not a text conflict"));
        }
        return Promise.resolve([{ kind: "Clean", content: "shared content" }]);
      });
      renderUncommitted(fakeClient({ getConflictHunks }), twoConflicts);

      await waitFor(() => screen.getByText("Keep our version"));
      await waitFor(() => screen.getByText("Save resolution"));

      // Both sections are mounted simultaneously now — one file's add/delete fallback state
      // must not appear on the other's section.
      expect(screen.getAllByText("Keep our version")).toHaveLength(1);
      expect(screen.getAllByText("Save resolution")).toHaveLength(1);
    });

    it("disables Commit while a Conflicted entry exists in status, even with staged files", () => {
      const mixedStatus: StatusEntry[] = [
        { path: "a.txt", staged: true, kind: "New" },
        { path: "shared.txt", staged: false, kind: "Conflicted" },
      ];
      renderUncommitted(fakeClient({ getConflictHunks: async () => [] }), mixedStatus);

      const commitButton = screen.getByText("Commit").closest("button");
      expect(commitButton).toBeDisabled();
    });

    it("groups unstaged and staged entries under labelled headings with counts", () => {
      renderUncommitted(fakeClient({}), status);

      expect(screen.getByText("Changes (1)")).toBeInTheDocument();
      expect(screen.getByText("Staged (1)")).toBeInTheDocument();
    });

    it("tints a conflicted file row and shows a conflict count badge", () => {
      const statusWithConflict: StatusEntry[] = [
        { path: "crates/git-core/src/merge.rs", staged: false, kind: "Conflicted" },
        { path: "README.md", staged: false, kind: "Modified" },
      ];
      renderUncommitted(fakeClient({ getConflictHunks: async () => [] }), statusWithConflict);

      const conflictedRow = screen.getByText("crates/git-core/src/merge.rs (Conflicted)").closest("li");
      expect(conflictedRow).toHaveClass(styles.conflicted);
      expect(screen.getByText("Changes (2)")).toBeInTheDocument();
      expect(screen.getByText("1 conflict")).toBeInTheDocument();
    });

    it("marks the current file's row as aria-selected on click, without hiding any other section", async () => {
      renderUncommitted(fakeClient({ getWorkingDiff: async () => [] }), status);

      const row = screen.getByText("a.txt (Modified)").closest('[role="option"]');
      expect(row).toHaveAttribute("aria-selected", "false");

      fireEvent.click(screen.getByText("a.txt (Modified)"));

      await waitFor(() => expect(row).toHaveAttribute("aria-selected", "true"));
      // The other file's section stays mounted and visible — clicking never hides it.
      expect(screen.getByText("b.txt (New)")).toBeInTheDocument();
    });

    it("renders a status-kind icon for each file row", () => {
      const { container } = render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={fakeClient({})}
          selectedRow="uncommitted"
          status={status}
          {...noopHandlers}
          mergeMessage={null}
          rebaseProgress={null}
        />,
      );

      // One status icon per file row (2 entries in `status`). Scoped to the rows so an
      // unrelated icon elsewhere in the pane can't satisfy this.
      expect(container.querySelectorAll('li[role="option"] svg').length).toBeGreaterThanOrEqual(2);
    });

    it("navigates the unstaged group with ArrowDown/ArrowUp, highlighting without hiding sections", async () => {
      const twoUnstaged: StatusEntry[] = [
        { path: "a.txt", staged: false, kind: "Modified" },
        { path: "c.txt", staged: false, kind: "New" },
      ];
      renderUncommitted(fakeClient({ getWorkingDiff: async () => [] }), twoUnstaged);

      const group = screen.getByRole("listbox", { name: "Unstaged changes" });
      fireEvent.keyDown(group, { key: "ArrowDown" });

      await waitFor(() =>
        expect(screen.getByText("a.txt (Modified)").closest('[role="option"]')).toHaveAttribute(
          "aria-selected",
          "true",
        ),
      );
      expect(screen.getByText("c.txt (New)")).toBeInTheDocument();
    });

    // The per-row Stage/Unstage buttons live inside a `role="option"` row, where ARIA's
    // listbox pattern doesn't reliably expose them to assistive tech during arrow-key
    // navigation (see `primitives/ListRow.tsx`'s doc comment). `s` on the group container —
    // the listbox's single tab stop, which already owns `j`/`k`/arrow navigation — is the
    // keyboard-only path to the same action.
    it("stages the current unstaged row when 's' is pressed on the group", async () => {
      const onStageFile = vi.fn();
      renderUncommitted(fakeClient({ getWorkingDiff: async () => [] }), status, { onStageFile });

      const group = screen.getByRole("listbox", { name: "Unstaged changes" });
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

    it("unstages the current staged row when 's' is pressed on the staged group", async () => {
      const onUnstageFile = vi.fn();
      renderUncommitted(fakeClient({ getWorkingDiff: async () => [] }), status, { onUnstageFile });

      const group = screen.getByRole("listbox", { name: "Staged changes" });
      fireEvent.keyDown(group, { key: "ArrowDown" });
      await waitFor(() =>
        expect(screen.getByText("b.txt (New)").closest('[role="option"]')).toHaveAttribute("aria-selected", "true"),
      );

      fireEvent.keyDown(group, { key: "s" });

      expect(onUnstageFile).toHaveBeenCalledWith("b.txt");
    });

    it("shows RebaseProgressPanel instead of CommitBox while a rebase is in progress", () => {
      renderUncommitted(fakeClient({}), status, { rebaseProgress: { currentStep: 1, totalSteps: 3 } });

      expect(screen.getByText(/Step 1 of 3/)).toBeInTheDocument();
      expect(screen.queryByPlaceholderText("Commit message")).not.toBeInTheDocument();
    });

    describe("diff loading", () => {
      it("shows a loading placeholder, not 'No text differences', while the diff is pending", async () => {
        const getWorkingDiff = vi.fn(() => new Promise<DiffHunk[]>(() => {}));
        renderUncommitted(fakeClient({ getWorkingDiff }), status);

        expect((await screen.findAllByText(/Loading diff/)).length).toBeGreaterThan(0);
        expect(screen.queryByText(/No text differences/)).not.toBeInTheDocument();
      });

      it("does not fetch a collapsed section's diff until it is expanded", async () => {
        const getWorkingDiff = vi.fn(async () => [] as DiffHunk[]);
        renderUncommitted(fakeClient({ getWorkingDiff }), status);
        await screen.findByRole("button", { name: "Collapse a.txt" });
        fireEvent.click(screen.getByRole("button", { name: "Collapse all" }));
        await waitFor(() => expect(screen.queryByText(/No text differences/)).not.toBeInTheDocument());
        getWorkingDiff.mockClear();

        fireEvent.click(screen.getByRole("button", { name: "Expand a.txt" }));

        await waitFor(() => expect(getWorkingDiff).toHaveBeenCalledTimes(1));
      });

      it("does not fetch an expanded file's diff until it is near the viewport (PERF-001)", () => {
        const observedCallbacks: IntersectionObserverCallback[] = [];
        const observe = vi.fn();
        const disconnect = vi.fn();
        // A plain `function`, not an arrow function: `new IntersectionObserver(...)` in the
        // component requires the mock to be constructible, which an arrow-function
        // implementation (no `[[Construct]]`) is not.
        const IntersectionObserverMock = vi.fn().mockImplementation(function (
          callback: IntersectionObserverCallback,
        ) {
          observedCallbacks.push(callback);
          return { observe, unobserve: vi.fn(), disconnect };
        });
        vi.stubGlobal("IntersectionObserver", IntersectionObserverMock);

        const getWorkingDiff = vi.fn(async () => [] as DiffHunk[]);
        const singleFile: StatusEntry[] = [{ path: "a.txt", staged: false, kind: "Modified" }];
        renderUncommitted(fakeClient({ getWorkingDiff }), singleFile);

        expect(observe).toHaveBeenCalledTimes(1);
        expect(getWorkingDiff).not.toHaveBeenCalled();

        act(() => {
          observedCallbacks[0](
            [{ isIntersecting: true } as IntersectionObserverEntry],
            {} as IntersectionObserver,
          );
        });

        expect(getWorkingDiff).toHaveBeenCalled();

        vi.unstubAllGlobals();
      });
    });

    describe("collapse", () => {
      it("collapsing a file's section hides its diff but keeps the header", async () => {
        const hunks: DiffHunk[] = [
          { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "changed line" }] },
        ];
        // Only a.txt gets this content — b.txt stays empty so it can't keep the text on screen
        // after a.txt's section collapses.
        const getWorkingDiff = vi.fn(async (_repoPath: string, path: string) => (path === "a.txt" ? hunks : []));
        renderUncommitted(fakeClient({ getWorkingDiff }), status);

        await screen.findByText("changed line");
        fireEvent.click(screen.getByRole("button", { name: "Collapse a.txt" }));

        expect(screen.queryByText("changed line")).not.toBeInTheDocument();
        expect(screen.getByText("a.txt (Modified)")).toBeInTheDocument();
      });

      it("expanding a collapsed section shows its diff again", async () => {
        const hunks: DiffHunk[] = [
          { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "changed line" }] },
        ];
        const getWorkingDiff = vi.fn(async (_repoPath: string, path: string) => (path === "a.txt" ? hunks : []));
        renderUncommitted(fakeClient({ getWorkingDiff }), status);

        await screen.findByText("changed line");
        fireEvent.click(screen.getByRole("button", { name: "Collapse a.txt" }));
        fireEvent.click(screen.getByRole("button", { name: "Expand a.txt" }));

        expect(await screen.findByText("changed line")).toBeInTheDocument();
      });

      it("Collapse all hides every section's body", async () => {
        renderUncommitted(fakeClient({ getWorkingDiff: async () => [] }), status);
        await screen.findByRole("button", { name: "Collapse a.txt" });

        fireEvent.click(screen.getByRole("button", { name: "Collapse all" }));

        expect(screen.queryByRole("button", { name: "Collapse a.txt" })).not.toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Expand a.txt" })).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Expand b.txt" })).toBeInTheDocument();
        // Headers stay, so the overview is still readable after collapsing everything.
        expect(screen.getByText("a.txt (Modified)")).toBeInTheDocument();
        expect(screen.getByText("b.txt (New)")).toBeInTheDocument();
      });

      it("Expand all (shown once everything is collapsed) reopens every section", async () => {
        renderUncommitted(fakeClient({ getWorkingDiff: async () => [] }), status);
        await screen.findByRole("button", { name: "Collapse a.txt" });
        fireEvent.click(screen.getByRole("button", { name: "Collapse all" }));

        fireEvent.click(screen.getByRole("button", { name: "Expand all" }));

        expect(screen.getByRole("button", { name: "Collapse a.txt" })).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Collapse b.txt" })).toBeInTheDocument();
      });
    });
  });

  describe("commit", () => {
    const getCommitFiles = vi.fn(async () => ["src/main.rs"]);
    const hunks: DiffHunk[] = [
      { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "fn main() {}" }] },
    ];
    const getCommitDiff = vi.fn(async () => hunks);

    it("renders every changed file's diff expanded by default, with no click needed", async () => {
      renderCommit(fakeClient({ getCommitFiles, getCommitDiff }), "abc123");

      expect(await screen.findByText("src/main.rs")).toBeInTheDocument();
      expect(await screen.findByText(/fn main/)).toBeInTheDocument();
      expect(getCommitDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "abc123", "src/main.rs", DEFAULT_DIFF_OPTIONS);
    });

    it("no CommitBox or stage/unstage buttons render", async () => {
      renderCommit(fakeClient({ getCommitFiles, getCommitDiff }), "abc123");

      expect(await screen.findByText("src/main.rs")).toBeInTheDocument();
      expect(screen.queryByText("Commit")).toBeNull();
      expect(screen.queryByText("Stage")).toBeNull();
    });

    it("no Stash button renders for a commit's diff", async () => {
      renderCommit(fakeClient({ getCommitFiles, getCommitDiff }), "abc123");

      expect(await screen.findByText("src/main.rs")).toBeInTheDocument();
      expect(screen.queryByText("Stash")).toBeNull();
    });

    it("renders a Blame button per file", async () => {
      renderCommit(fakeClient({ getCommitFiles, getCommitDiff }), "abc123");

      expect(await screen.findByText("src/main.rs")).toBeInTheDocument();
      expect(screen.getByText("Blame")).toBeInTheDocument();
    });

    it("clicking Blame fetches and renders blame for that commit's file", async () => {
      const blameLines: BlameLine[] = [
        { lineNumber: 1, content: "fn main() {}", commitId: "abc123", shortId: "abc1234", authorName: "Rene", timestamp: 1 },
      ];
      const getBlame = vi.fn(async () => blameLines);
      renderCommit(fakeClient({ getCommitFiles, getCommitDiff, getBlame }), "abc123");

      await screen.findByText("src/main.rs");
      fireEvent.click(screen.getByText("Blame"));

      expect(await screen.findByText("fn main() {}")).toBeInTheDocument();
      expect(getBlame).toHaveBeenCalledWith(TEST_REPO_PATH, "abc123", "src/main.rs");
    });

    it("clicking a blame line calls onSelectRow with that line's commit id", async () => {
      const blameLines: BlameLine[] = [
        { lineNumber: 1, content: "fn main() {}", commitId: "abc123", shortId: "abc1234", authorName: "Rene", timestamp: 1 },
      ];
      const getBlame = vi.fn(async () => blameLines);
      const onSelectRow = vi.fn();
      renderCommit(fakeClient({ getCommitFiles, getCommitDiff, getBlame }), "abc123", { onSelectRow });

      await screen.findByText("src/main.rs");
      fireEvent.click(screen.getByText("Blame"));
      const row = await screen.findByText("fn main() {}");
      fireEvent.click(row.closest("tr")!);

      expect(onSelectRow).toHaveBeenCalledWith({ commitId: "abc123" });
    });

    it("shows a friendly message, not the raw error, when blame fetch rejects", async () => {
      const getBlame = vi.fn(async () => {
        throw new Error("git operation failed: the path 'src/main.rs' does not exist in the given tree");
      });
      renderCommit(fakeClient({ getCommitFiles, getCommitDiff, getBlame }), "abc123");

      await screen.findByText("src/main.rs");
      fireEvent.click(screen.getByText("Blame"));

      expect(await screen.findByText("No blame available for this file at this revision.")).toBeInTheDocument();
      expect(screen.queryByText(/does not exist in the given tree/)).not.toBeInTheDocument();
    });

    it("shows the raw error and a Retry that re-fetches when a commit file's diff fails", async () => {
      const getCommitDiffOnce = vi.fn<RepoClient["getCommitDiff"]>().mockResolvedValue([]);
      getCommitDiffOnce.mockRejectedValueOnce(new Error("failed to read working diff: permission denied"));
      renderCommit(fakeClient({ getCommitFiles, getCommitDiff: getCommitDiffOnce }), "abc123");

      expect(
        await screen.findByText("failed to read working diff: permission denied"),
      ).toBeInTheDocument();
      expect(
        screen.getByText("Could not read this file's changes. Retry, or check the file still exists."),
      ).toBeInTheDocument();
      expect(getCommitDiffOnce).toHaveBeenCalledTimes(1);

      fireEvent.click(screen.getByRole("button", { name: "Retry" }));

      await waitFor(() => expect(getCommitDiffOnce).toHaveBeenCalledTimes(2));
      expect(
        screen.queryByText("failed to read working diff: permission denied"),
      ).not.toBeInTheDocument();
    });

    it("shows a Retry that re-fetches when a commit's file list fails to load", async () => {
      const getCommitFilesOnce = vi.fn<RepoClient["getCommitFiles"]>().mockResolvedValue(["src/main.rs"]);
      getCommitFilesOnce.mockRejectedValueOnce(new Error("something unusual"));
      renderCommit(fakeClient({ getCommitFiles: getCommitFilesOnce, getCommitDiff }), "abc123");

      expect(await screen.findByText("something unusual")).toBeInTheDocument();
      expect(getCommitFilesOnce).toHaveBeenCalledTimes(1);

      fireEvent.click(screen.getByRole("button", { name: "Retry" }));

      await waitFor(() => expect(getCommitFilesOnce).toHaveBeenCalledTimes(2));
      expect(await screen.findByText("src/main.rs")).toBeInTheDocument();
    });

    it("Collapse all / Expand all toggles every file's section", async () => {
      const getTwoCommitFiles = vi.fn(async () => ["src/main.rs", "src/lib.rs"]);
      renderCommit(fakeClient({ getCommitFiles: getTwoCommitFiles, getCommitDiff }), "abc123");

      await screen.findByRole("button", { name: "Collapse src/main.rs" });
      fireEvent.click(screen.getByRole("button", { name: "Collapse all" }));

      expect(screen.getByRole("button", { name: "Expand src/main.rs" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Expand src/lib.rs" })).toBeInTheDocument();

      fireEvent.click(screen.getByRole("button", { name: "Expand all" }));

      expect(screen.getByRole("button", { name: "Collapse src/main.rs" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Collapse src/lib.rs" })).toBeInTheDocument();
    });
  });
});

describe("DiffPane line staging", () => {
  const lineHunk: DiffHunk = {
    oldStart: 1, oldLines: 3, newStart: 1, newLines: 4,
    lines: [
      { origin: "Context", content: "a\n" },
      { origin: "Remove", content: "b\n" },
      { origin: "Add", content: "B\n" },
      { origin: "Context", content: "c\n" },
      { origin: "Add", content: "d\n" },
    ],
  };

  const TestDiffSettings = ({ ignoreWhitespace, children }: { ignoreWhitespace: boolean; children: ReactNode }) => (
    <DiffSettingsContext.Provider
      value={{
        settings: { ignoreWhitespace, contextLines: 3, syntaxHighlight: true },
        update: () => {},
        error: null,
        dismissError: () => {},
      }}
    >
      {children}
    </DiffSettingsContext.Provider>
  );

  it("stages a clicked line with the path, hunk starts, context lines and line index", async () => {
    const onStageLines = vi.fn();
    const { container } = renderUncommitted(
      fakeClient({ getWorkingDiff: async () => [lineHunk] }),
      [{ path: "f.txt", staged: false, kind: "Modified" }],
      { onStageLines },
    );
    await screen.findByText("Stage hunk");
    fireEvent.click(container.querySelector<HTMLElement>('[data-hunk-index="0"][data-line-index="2"]')!);
    fireEvent.click(screen.getByRole("button", { name: "Stage lines (1)" }));
    await waitFor(() => expect(onStageLines).toHaveBeenCalledWith("f.txt", 1, 1, 3, [2], ["a\n", "b\n", "B\n", "c\n", "d\n"]));
  });

  it("a staged section never discards, with or without a line selection", async () => {
    const onDiscardHunk = vi.fn();
    const onDiscardLines = vi.fn();
    const { container } = renderUncommitted(
      fakeClient({ getWorkingDiff: async () => [lineHunk] }),
      [{ path: "f.txt", staged: true, kind: "Modified" }],
      { onDiscardHunk, onDiscardLines },
    );
    await screen.findByText("Unstage hunk");
    expect(screen.queryByRole("button", { name: "Discard hunk" })).toBeNull();
    const group = screen.getByRole("group", { name: "Diff hunks" });
    fireEvent.keyDown(group, { key: "d" });
    fireEvent.keyDown(group, { key: "d" });
    fireEvent.click(container.querySelector<HTMLElement>('[data-hunk-index="0"][data-line-index="2"]')!);
    fireEvent.keyDown(group, { key: "d" });
    fireEvent.keyDown(group, { key: "d" });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(onDiscardHunk).not.toHaveBeenCalled();
    expect(onDiscardLines).not.toHaveBeenCalled();
  });

  it("ignore whitespace hides hunk actions in both sections but keeps file-level staging", async () => {
    render(
      <TestDiffSettings ignoreWhitespace>
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={fakeClient({ getWorkingDiff: async () => [lineHunk] })}
          selectedRow="uncommitted"
          status={[
            { path: "f.txt", staged: false, kind: "Modified" },
            { path: "f.txt", staged: true, kind: "Modified" },
          ]}
          mergeMessage={null}
          rebaseProgress={null}
          {...noopHandlers}
        />
      </TestDiffSettings>,
    );
    expect(await screen.findAllByText("Turn off Ignore whitespace to stage partially.")).toHaveLength(2);
    await waitFor(() => expect(screen.getAllByText("B")).toHaveLength(2));
    expect(screen.queryByRole("button", { name: "Stage hunk" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Unstage hunk" })).toBeNull();
    expect(screen.getByRole("button", { name: "Stage f.txt" })).toBeInTheDocument();
  });

  it("commit diffs show lines and no partial-staging hint", async () => {
    render(
      <TestDiffSettings ignoreWhitespace>
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={fakeClient({ getCommitFiles: async () => ["f.txt"], getCommitDiff: async () => [lineHunk], getCommitMessage: async () => "" })}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          mergeMessage={null}
          rebaseProgress={null}
          {...noopHandlers}
        />
      </TestDiffSettings>,
    );
    expect(await screen.findByText("B")).toBeInTheDocument();
    expect(screen.queryByText("Turn off Ignore whitespace to stage partially.")).toBeNull();
  });
});

describe("DiffPane diff options contract", () => {
  const lineHunk: DiffHunk = {
    oldStart: 1, oldLines: 3, newStart: 1, newLines: 4,
    lines: [
      { origin: "Context", content: "a\n" },
      { origin: "Remove", content: "b\n" },
      { origin: "Add", content: "B\n" },
      { origin: "Context", content: "c\n" },
      { origin: "Add", content: "d\n" },
    ],
  };
  const shown = lineHunk.lines.map((l) => l.content);
  const status: StatusEntry[] = [{ path: "f.txt", staged: false, kind: "Modified" }];

  function pane(
    client: RepoClient,
    settings: { ignoreWhitespace: boolean; contextLines: number },
    handlers: Partial<React.ComponentProps<typeof DiffPane>>,
  ) {
    return (
      <DiffSettingsContext.Provider
        value={{ settings: { ...settings, syntaxHighlight: false }, update: () => {}, error: null, dismissError: () => {} }}
      >
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          mergeMessage={null}
          rebaseProgress={null}
          {...noopHandlers}
          {...handlers}
        />
      </DiffSettingsContext.Provider>
    );
  }

  function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((r) => {
      resolve = r;
    });
    return { promise, resolve };
  }

  it("fetches with non-default options and forwards their context lines to hunk and line actions", async () => {
    const getWorkingDiff = vi.fn(async () => [lineHunk]);
    const onStageHunk = vi.fn();
    const onStageLines = vi.fn();
    const { container } = render(
      pane(fakeClient({ getWorkingDiff }), { ignoreWhitespace: false, contextLines: 7 }, { onStageHunk, onStageLines }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Stage hunk" }));
    expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "f.txt", false, { ignoreWhitespace: false, contextLines: 7 });
    await waitFor(() => expect(onStageHunk).toHaveBeenCalledWith("f.txt", 1, 1, 7));

    fireEvent.click(container.querySelector<HTMLElement>('[data-hunk-index="0"][data-line-index="2"]')!);
    fireEvent.click(screen.getByRole("button", { name: "Stage lines (1)" }));
    await waitFor(() => expect(onStageLines).toHaveBeenCalledWith("f.txt", 1, 1, 7, [2], shown));
  });

  it("offers no hunk or line action while a context-lines change is refetching, then uses the new value", async () => {
    const pending = deferred<DiffHunk[]>();
    const getWorkingDiff = vi
      .fn<RepoClient["getWorkingDiff"]>()
      .mockResolvedValue([lineHunk])
      .mockResolvedValueOnce([lineHunk])
      .mockReturnValueOnce(pending.promise);
    const onStageHunk = vi.fn();
    const client = fakeClient({ getWorkingDiff });
    const { rerender, container } = render(pane(client, { ignoreWhitespace: false, contextLines: 3 }, { onStageHunk }));
    await screen.findByRole("button", { name: "Stage hunk" });

    rerender(pane(client, { ignoreWhitespace: false, contextLines: 7 }, { onStageHunk }));

    expect(getWorkingDiff).toHaveBeenLastCalledWith(TEST_REPO_PATH, "f.txt", false, { ignoreWhitespace: false, contextLines: 7 });
    expect(screen.queryByRole("button", { name: "Stage hunk" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Discard hunk" })).toBeNull();
    expect(screen.queryByRole("group", { name: "Diff hunks" })).toBeNull();
    fireEvent.click(container.querySelector<HTMLElement>('[data-hunk-index="0"][data-line-index="2"]')!);
    expect(screen.queryByRole("button", { name: /^(Stage|Unstage|Discard) lines/ })).toBeNull();

    await act(async () => pending.resolve([lineHunk]));
    fireEvent.click(await screen.findByRole("button", { name: "Stage hunk" }));
    await waitFor(() => expect(onStageHunk).toHaveBeenCalledWith("f.txt", 1, 1, 7));
    expect(onStageHunk).toHaveBeenCalledTimes(1);
  });

  it("keeps partial actions off until hunks fetched without ignore-whitespace arrive", async () => {
    const pending = deferred<DiffHunk[]>();
    const getWorkingDiff = vi
      .fn<RepoClient["getWorkingDiff"]>()
      .mockResolvedValue([lineHunk])
      .mockResolvedValueOnce([lineHunk])
      .mockReturnValueOnce(pending.promise);
    const client = fakeClient({ getWorkingDiff });
    const { rerender } = render(pane(client, { ignoreWhitespace: true, contextLines: 3 }, {}));
    expect(await screen.findByText("Turn off Ignore whitespace to stage partially.")).toBeInTheDocument();

    rerender(pane(client, { ignoreWhitespace: false, contextLines: 3 }, {}));
    expect(screen.queryByRole("button", { name: "Stage hunk" })).toBeNull();

    await act(async () => pending.resolve([lineHunk]));
    expect(await screen.findByRole("button", { name: "Stage hunk" })).toBeInTheDocument();
    expect(screen.queryByText("Turn off Ignore whitespace to stage partially.")).toBeNull();
  });

  it("hides partial actions as soon as ignore-whitespace is turned on, before the refetch lands", async () => {
    const pending = deferred<DiffHunk[]>();
    const getWorkingDiff = vi
      .fn<RepoClient["getWorkingDiff"]>()
      .mockResolvedValue([lineHunk])
      .mockResolvedValueOnce([lineHunk])
      .mockReturnValueOnce(pending.promise);
    const client = fakeClient({ getWorkingDiff });
    const { rerender } = render(pane(client, { ignoreWhitespace: false, contextLines: 3 }, {}));
    await screen.findByRole("button", { name: "Stage hunk" });

    rerender(pane(client, { ignoreWhitespace: true, contextLines: 3 }, {}));
    expect(screen.queryByRole("button", { name: "Stage hunk" })).toBeNull();

    await act(async () => pending.resolve([lineHunk]));
    expect(await screen.findByText("Turn off Ignore whitespace to stage partially.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Stage hunk" })).toBeNull();
  });

  describe("file history", () => {
    it("offers History on tracked working-tree files only", () => {
      const onOpenFileHistory = vi.fn();
      renderUncommitted(
        fakeClient({}),
        [
          { path: "a.txt", staged: false, kind: "Modified" },
          { path: "new.txt", staged: false, kind: "New" },
        ],
        { onOpenFileHistory },
      );

      fireEvent.click(screen.getByRole("button", { name: "History of a.txt" }));
      expect(onOpenFileHistory).toHaveBeenCalledWith("a.txt");
      expect(screen.queryByRole("button", { name: "History of new.txt" })).toBeNull();
    });

    it("offers History on commit files", async () => {
      const onOpenFileHistory = vi.fn();
      renderCommit(
        fakeClient({ getCommitFiles: async () => ["f.txt"], getCommitMessage: async () => "" }),
        "abc123",
        { onOpenFileHistory },
      );

      fireEvent.click(await screen.findByRole("button", { name: "History of f.txt" }));
      expect(onOpenFileHistory).toHaveBeenCalledWith("f.txt");
    });

    it("shows only the history file in focus mode, with a way to show all files", async () => {
      renderCommit(
        fakeClient({ getCommitFiles: async () => ["f.txt", "g.txt", "h.txt"], getCommitMessage: async () => "" }),
        "abc123",
        { fileHistoryFocus: { path: "g.txt", oldPath: null } },
      );

      expect(await screen.findByText("g.txt")).toBeInTheDocument();
      expect(screen.queryByText("f.txt")).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: "Show all 3 files" }));
      expect(await screen.findByText("f.txt")).toBeInTheDocument();
    });

    it("offers no History button on the focused file, since its history is already open", async () => {
      renderCommit(
        fakeClient({ getCommitFiles: async () => ["f.txt", "g.txt"], getCommitMessage: async () => "" }),
        "abc123",
        { fileHistoryFocus: { path: "g.txt", oldPath: null }, onOpenFileHistory: vi.fn() },
      );

      expect(await screen.findByText("g.txt")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "History of g.txt" })).toBeNull();
    });

    it("collapses only the focused file from its toggle, without revealing other files", async () => {
      renderCommit(
        fakeClient({ getCommitFiles: async () => ["f.txt", "g.txt"], getCommitMessage: async () => "" }),
        "abc123",
        { fileHistoryFocus: { path: "g.txt", oldPath: null } },
      );

      fireEvent.click(await screen.findByRole("button", { name: "Collapse g.txt" }));

      expect(screen.getByRole("button", { name: "Expand g.txt" })).toBeInTheDocument();
      expect(screen.queryByText("f.txt")).toBeNull();
    });

    it("uses the rename diff and labels the old path for a rename entry", async () => {
      const getCommitRenameDiff = vi.fn(async () => []);
      const getCommitDiff = vi.fn(async () => []);
      renderCommit(
        fakeClient({
          getCommitFiles: async () => ["old.txt", "new.txt"],
          getCommitMessage: async () => "",
          getCommitRenameDiff,
          getCommitDiff,
        }),
        "abc123",
        { fileHistoryFocus: { path: "new.txt", oldPath: "old.txt" } },
      );

      expect(await screen.findByText("Renamed from old.txt")).toBeInTheDocument();
      await waitFor(() =>
        expect(getCommitRenameDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "abc123", "old.txt", "new.txt", expect.anything()),
      );
      expect(getCommitDiff).not.toHaveBeenCalled();
    });
  });
  describe("commit header signature state", () => {
    it("resets to checking on commit switch and drops a late result for the old commit", async () => {
      const sig = (status: "good" | "bad") => ({ status, format: "ssh" as const, signer: null, key: null, detail: null });
      let resolveA: (value: ReturnType<typeof sig>) => void = () => {};
      const client = fakeClient({
        getCommitFiles: async () => [],
        getCommitMessage: async () => "",
        getCommitSignature: (_repo: string, id: string) =>
          id === "aaa111" ? new Promise((resolve) => (resolveA = resolve)) : id === "bbb222" ? Promise.resolve(sig("good")) : new Promise(() => {}),
      });
      const element = (commitId: string) => (
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow={{ commitId }}
          status={[]}
          mergeMessage={null}
          rebaseProgress={null}
          {...noopHandlers}
        />
      );
      const view = rtlRender(element("aaa111"));
      expect(await screen.findByText("Checking signature…")).toBeInTheDocument();

      view.rerender(element("bbb222"));
      expect(await screen.findByText("Good signature")).toBeInTheDocument();

      view.rerender(element("ccc333"));
      // Synchronously after the switch: the previous commit's status must already be gone.
      expect(screen.queryByText("Good signature")).not.toBeInTheDocument();
      expect(screen.getByText("Checking signature…")).toBeInTheDocument();

      await act(async () => resolveA(sig("bad")));
      expect(screen.queryByText("Bad signature")).not.toBeInTheDocument();
      expect(screen.getByText("Checking signature…")).toBeInTheDocument();
    });
  });
});

describe("DiffPane stash view", () => {
  const stash = { index: 0, message: "On main: wip", commitId: "s0" };
  const stashActions = (overrides: Partial<NonNullable<React.ComponentProps<typeof DiffPane>["stashActions"]>> = {}) => ({
    onApply: vi.fn(),
    onPop: vi.fn(),
    onDrop: vi.fn(),
    restoreIndex: true,
    onSetRestoreIndex: vi.fn(),
    disabledReason: null,
    ...overrides,
  });

  function renderStash(
    files: { tracked: string[]; untracked: string[]; untrackedCommitId: string | null },
    actions = stashActions(),
  ) {
    const getCommitDiff = vi.fn(async () => []);
    const client = fakeClient({
      getStashFiles: vi.fn(async () => files),
      getCommitDiff,
      getBlame: async () => [],
    });
    renderCommit(client, "s0", { stashes: [stash], stashActions: actions });
    return { getCommitDiff, client, actions };
  }

  it("shows the stash's changes and untracked files, each diffed against its own commit", async () => {
    const { getCommitDiff, client } = renderStash({ tracked: ["a.txt"], untracked: ["new.txt"], untrackedCommitId: "u1" });

    const section = await screen.findByRole("region", { name: "Stash details" });
    expect(within(section).getByRole("heading", { name: "On main: wip" })).toBeInTheDocument();
    expect(section).toHaveTextContent("stash@{0}");
    expect(await within(section).findByText("Changes (1)")).toBeInTheDocument();
    expect(within(section).getByText("Untracked (1)")).toBeInTheDocument();
    await waitFor(() => {
      expect(getCommitDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "s0", "a.txt", expect.anything());
      expect(getCommitDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "u1", "new.txt", expect.anything());
    });
    expect(client.getStashFiles).toHaveBeenCalledWith(TEST_REPO_PATH, "s0");
    expect(within(section).queryByRole("button", { name: /History of/ })).toBeNull();
  });

  it("hides the Untracked group when the stash has none", async () => {
    renderStash({ tracked: ["a.txt"], untracked: [], untrackedCommitId: null });

    expect(await screen.findByText("Changes (1)")).toBeInTheDocument();
    expect(screen.queryByText(/Untracked \(/)).toBeNull();
  });

  it("says when a stash has no file changes", async () => {
    renderStash({ tracked: [], untracked: [], untrackedCommitId: null });

    expect(await screen.findByText("This stash has no file changes.")).toBeInTheDocument();
  });

  it("applies, pops and drops (after confirming) from the toolbar", async () => {
    const { actions } = renderStash({ tracked: ["a.txt"], untracked: [], untrackedCommitId: null });
    const section = await screen.findByRole("region", { name: "Stash details" });

    fireEvent.click(within(section).getByRole("button", { name: "Apply" }));
    expect(actions.onApply).toHaveBeenCalledWith(0);
    fireEvent.click(within(section).getByRole("button", { name: "Pop" }));
    expect(actions.onPop).toHaveBeenCalledWith(0);

    fireEvent.click(within(section).getByRole("button", { name: "Drop…" }));
    expect(actions.onDrop).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog", { name: "Drop stash On main: wip", hidden: true });
    expect(dialog).toHaveTextContent('Drop "On main: wip"? This cannot be undone.');
    fireEvent.click(within(dialog).getByRole("button", { name: "Drop stash", hidden: true }));
    expect(actions.onDrop).toHaveBeenCalledWith(0);
  });

  it("reflects and toggles Restore staged changes", async () => {
    const { actions } = renderStash({ tracked: ["a.txt"], untracked: [], untrackedCommitId: null });
    const checkbox = await screen.findByRole("checkbox", { name: "Restore staged changes" });

    expect(checkbox).toBeChecked();
    fireEvent.click(checkbox);

    expect(actions.onSetRestoreIndex).toHaveBeenCalledWith(false);
  });

  it("disables Apply and Pop with the reason, keeping Drop", async () => {
    renderStash(
      { tracked: ["a.txt"], untracked: [], untrackedCommitId: null },
      stashActions({ disabledReason: "Unstage or commit staged changes first." }),
    );
    const section = await screen.findByRole("region", { name: "Stash details" });

    for (const name of ["Apply", "Pop"]) {
      expect(within(section).getByRole("button", { name })).toBeDisabled();
      expect(within(section).getByRole("button", { name })).toHaveAttribute("title", "Unstage or commit staged changes first.");
    }
    expect(within(section).getByRole("button", { name: "Drop…" })).toBeEnabled();
  });

  it("shows a load failure with Retry", async () => {
    let fail = true;
    const client = fakeClient({
      getStashFiles: async () => {
        if (fail) throw new Error("No stash with id s0.");
        return { tracked: ["a.txt"], untracked: [], untrackedCommitId: null };
      },
      getCommitDiff: async () => [],
    });
    renderCommit(client, "s0", { stashes: [stash], stashActions: stashActions() });

    expect(await screen.findByText(/No stash with id s0\./)).toBeInTheDocument();
    fail = false;
    fireEvent.click(screen.getByRole("button", { name: /Retry/ }));
    expect(await screen.findByText("Changes (1)")).toBeInTheDocument();
  });
});

describe("DiffPane external tools", () => {
  function tools(overrides: Partial<ExternalToolsApi> = {}): ExternalToolsApi {
    return {
      openInEditor: vi.fn(),
      reveal: vi.fn(),
      openTerminal: vi.fn(),
      openDiffTool: vi.fn(),
      openMergeTool: vi.fn(),
      mergeToolRunning: new Set(),
      mergeToolResults: new Map(),
      dismissMergeToolResult: vi.fn(),
      error: null,
      dismissError: vi.fn(),
      ...overrides,
    };
  }
  const pick = (path: string, label: string) => {
    fireEvent.click(screen.getByRole("button", { name: `Actions for ${path}` }));
    fireEvent.click(screen.getByRole("menuitem", { name: label }));
  };
  const conflictClient = () => fakeClient({ getConflictHunks: async () => [] });

  it("opens a modified file in the editor at its first hunk line and in the diff tool", async () => {
    const hunks: DiffHunk[] = [
      { oldStart: 12, oldLines: 1, newStart: 14, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
    ];
    const externalTools = tools();
    renderUncommitted(fakeClient({ getWorkingDiff: async () => hunks }), [{ path: "a.ts", staged: false, kind: "Modified" }], {
      externalTools,
    });
    await screen.findByText(/^x$/);

    pick("a.ts", "Open in editor");
    expect(externalTools.openInEditor).toHaveBeenCalledWith("a.ts", 14);
    pick("a.ts", "Reveal in file manager");
    expect(externalTools.reveal).toHaveBeenCalledWith("a.ts");
    pick("a.ts", "Open in diff tool");
    expect(externalTools.openDiffTool).toHaveBeenCalledWith("a.ts", { kind: "Unstaged" });
  });

  it("opens at no particular line when the first hunk starts at line 0 (the file was emptied)", async () => {
    const hunks: DiffHunk[] = [
      { oldStart: 1, oldLines: 1, newStart: 0, newLines: 0, lines: [{ origin: "Remove", content: "gone" }] },
    ];
    const externalTools = tools();
    renderUncommitted(fakeClient({ getWorkingDiff: async () => hunks }), [{ path: "a.ts", staged: false, kind: "Modified" }], {
      externalTools,
    });
    await screen.findByText(/^gone$/);
    pick("a.ts", "Open in editor");
    expect(externalTools.openInEditor).toHaveBeenCalledWith("a.ts", null);
  });

  it("a staged entry opens the staged diff", () => {
    const externalTools = tools();
    renderUncommitted(fakeClient({}), [{ path: "a.ts", staged: true, kind: "Modified" }], { externalTools });
    pick("a.ts", "Open in diff tool");
    expect(externalTools.openDiffTool).toHaveBeenCalledWith("a.ts", { kind: "Staged" });
  });

  it("a conflicted entry has a menu with the merge tool, disabled while it runs", () => {
    const externalTools = tools();
    const { unmount } = renderUncommitted(conflictClient(), [{ path: "c.txt", staged: false, kind: "Conflicted" }], {
      externalTools,
    });
    pick("c.txt", "Open in merge tool");
    expect(externalTools.openMergeTool).toHaveBeenCalledWith("c.txt");
    unmount();

    renderUncommitted(conflictClient(), [{ path: "c.txt", staged: false, kind: "Conflicted" }], {
      externalTools: tools({ mergeToolRunning: new Set(["c.txt"]) }),
    });
    fireEvent.click(screen.getByRole("button", { name: "Actions for c.txt" }));
    expect(screen.getByRole("menuitem", { name: "Open in merge tool" })).toBeDisabled();
  });

  it("shows the merge tool result and marks the file resolved", async () => {
    const externalTools = tools({ mergeToolResults: new Map([["c.txt", "Resolved"]]) });
    const onMarkResolved = vi.fn(async () => "Resolved" as const);
    renderUncommitted(conflictClient(), [{ path: "c.txt", staged: false, kind: "Conflicted" }], {
      externalTools,
      onMarkResolved,
    });

    expect(screen.getByRole("status")).toHaveTextContent("The merge tool finished.");
    fireEvent.click(within(screen.getByRole("status")).getByRole("button", { name: "Mark resolved" }));
    await waitFor(() => expect(externalTools.dismissMergeToolResult).toHaveBeenCalledWith("c.txt"));
    expect(onMarkResolved).toHaveBeenCalledWith("c.txt", false);
  });

  it("asks before marking a file with conflict markers resolved", async () => {
    const externalTools = tools({ mergeToolResults: new Map([["c.txt", "Resolved"]]) });
    const onMarkResolved = vi
      .fn<(path: string, force: boolean) => Promise<"Resolved" | "MarkersRemain" | null>>()
      .mockResolvedValueOnce("MarkersRemain")
      .mockResolvedValueOnce("Resolved");
    renderUncommitted(conflictClient(), [{ path: "c.txt", staged: false, kind: "Conflicted" }], {
      externalTools,
      onMarkResolved,
    });

    fireEvent.click(within(screen.getByRole("status")).getByRole("button", { name: "Mark resolved" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Conflict markers remain in c.txt. Mark it resolved anyway?");
    fireEvent.click(within(dialog).getByRole("button", { name: "Mark resolved" }));
    await waitFor(() => expect(onMarkResolved).toHaveBeenLastCalledWith("c.txt", true));
    await waitFor(() => expect(externalTools.dismissMergeToolResult).toHaveBeenCalledWith("c.txt"));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("an unresolved merge tool result still offers Mark resolved, guarded by the markers check", async () => {
    const externalTools = tools({ mergeToolResults: new Map([["c.txt", "Unresolved"]]) });
    const onMarkResolved = vi.fn(async () => "MarkersRemain" as const);
    renderUncommitted(conflictClient(), [{ path: "c.txt", staged: false, kind: "Conflicted" }], {
      externalTools,
      onMarkResolved,
    });

    const notice = screen.getByRole("status");
    expect(notice).toHaveTextContent("The merge tool closed without resolving the conflict.");
    fireEvent.click(within(notice).getByRole("button", { name: "Mark resolved" }));
    expect(onMarkResolved).toHaveBeenCalledWith("c.txt", false);
    expect(await screen.findByText("Conflict markers remain in c.txt. Mark it resolved anyway?")).toBeInTheDocument();
    fireEvent.click(within(notice).getByRole("button", { name: "Dismiss" }));
    expect(externalTools.dismissMergeToolResult).toHaveBeenCalledWith("c.txt");
  });

  it("the conflict pane's whole-file buttons and Mark resolved go through the section", async () => {
    const onMarkResolved = vi
      .fn<(path: string, force: boolean) => Promise<"Resolved" | "MarkersRemain" | null>>()
      .mockResolvedValueOnce("MarkersRemain")
      .mockResolvedValueOnce("Resolved");
    const onResolveWholeFile = vi.fn();
    renderUncommitted(conflictClient(), [{ path: "c.txt", staged: false, kind: "Conflicted" }], {
      onMarkResolved,
      onResolveWholeFile,
    });

    const header = await screen.findByRole("group", { name: "Whole file" });
    await waitFor(() => expect(within(header).getByRole("button", { name: "Use theirs (whole file)" })).toBeEnabled());
    fireEvent.click(within(header).getByRole("button", { name: "Use theirs (whole file)" }));
    expect(onResolveWholeFile).toHaveBeenCalledWith("c.txt", "Theirs");

    fireEvent.click(within(header).getByRole("button", { name: "Mark resolved" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Conflict markers remain in c.txt. Mark it resolved anyway?");
    fireEvent.click(within(dialog).getByRole("button", { name: "Mark resolved" }));
    await waitFor(() => expect(onMarkResolved).toHaveBeenLastCalledWith("c.txt", true));
  });

  it("commit file sections have an actions menu with a commit diff source", async () => {
    const externalTools = tools();
    renderCommit(fakeClient({ getCommitFiles: async () => ["b.ts"], getCommitDiff: async () => [] }), "abc123", {
      externalTools,
    });
    await screen.findByText("b.ts");

    pick("b.ts", "Open in diff tool");
    expect(externalTools.openDiffTool).toHaveBeenCalledWith("b.ts", { kind: "Commit", commitId: "abc123", oldPath: null });
    pick("b.ts", "Open in editor");
    expect(externalTools.openInEditor).toHaveBeenCalledWith("b.ts", null);

    fireEvent.contextMenu(screen.getByText("b.ts"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Reveal in file manager" }));
    expect(externalTools.reveal).toHaveBeenCalledWith("b.ts");
  });

  it("without externalTools the menus stay as before", async () => {
    renderUncommitted(conflictClient(), [{ path: "c.txt", staged: false, kind: "Conflicted" }]);
    expect(screen.queryByRole("button", { name: "Actions for c.txt" })).toBeNull();
    cleanup();
    renderCommit(fakeClient({ getCommitFiles: async () => ["b.ts"], getCommitDiff: async () => [] }), "abc123");
    await screen.findByText("b.ts");
    expect(screen.queryByRole("button", { name: "Actions for b.ts" })).toBeNull();
  });
});

describe("DiffPane image preview", () => {
  const pngSide = (base64: string) => ({ kind: "Loaded" as const, size: 3, base64, mime: "image/png" });
  const svgSide = { kind: "Loaded" as const, size: 8, base64: "PHN2Zy8+", mime: "image/svg+xml" };

  it("previews an unstaged image instead of fetching hunks", async () => {
    const getFileSides = vi.fn(async () => ({ old: pngSide("Zm9v"), new: pngSide("YmFy") }));
    const getWorkingDiff = vi.fn(async () => []);
    renderUncommitted(fakeClient({ getFileSides, getWorkingDiff }), [{ path: "logo.png", staged: false, kind: "Modified" }]);

    expect(await screen.findByRole("img", { name: "After: logo.png" })).toHaveAttribute("src", "data:image/png;base64,YmFy");
    expect(getFileSides).toHaveBeenCalledWith(TEST_REPO_PATH, "logo.png", { kind: "Unstaged" });
    expect(getWorkingDiff).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Show text diff" })).toBeNull();
  });

  it("previews a staged image against HEAD", async () => {
    const getFileSides = vi.fn(async () => ({ old: { kind: "Missing" as const }, new: pngSide("YmFy") }));
    renderUncommitted(fakeClient({ getFileSides, getWorkingDiff: vi.fn(async () => []) }), [
      { path: "logo.png", staged: true, kind: "New" },
    ]);
    expect(await screen.findByText("Added")).toBeInTheDocument();
    expect(getFileSides).toHaveBeenCalledWith(TEST_REPO_PATH, "logo.png", { kind: "Staged" });
  });

  it("lets an SVG switch between the image and its text diff", async () => {
    const getFileSides = vi.fn(async () => ({ old: { kind: "Missing" as const }, new: svgSide }));
    const getWorkingDiff = vi.fn(async (): Promise<DiffHunk[]> => [
      { oldStart: 0, oldLines: 0, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "<svg/>" }] },
    ]);
    renderUncommitted(fakeClient({ getFileSides, getWorkingDiff }), [{ path: "icon.svg", staged: false, kind: "New" }]);

    expect(await screen.findByRole("img", { name: "After: icon.svg" })).toBeInTheDocument();
    expect(getWorkingDiff).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Show text diff" }));
    await waitFor(() => expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "icon.svg", false, DEFAULT_DIFF_OPTIONS));
    expect(await screen.findByText("<svg/>")).toBeInTheDocument();
    expect(screen.queryByRole("img")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show image" }));
    expect(await screen.findByRole("img", { name: "After: icon.svg" })).toBeInTheDocument();
  });

  it("sends a conflicted image to the conflict pane, not the preview", async () => {
    const getFileSides = vi.fn();
    renderUncommitted(
      fakeClient({ getFileSides, getConflictHunks: vi.fn(async () => []), getWorkingDiff: vi.fn(async () => []) }),
      [{ path: "logo.png", staged: false, kind: "Conflicted" }],
    );
    await screen.findByText(/logo\.png/);
    expect(getFileSides).not.toHaveBeenCalled();
  });

  it("previews a committed image with the commit and its rename", async () => {
    const getFileSides = vi.fn(async () => ({ old: pngSide("Zm9v"), new: pngSide("YmFy") }));
    const getCommitDiff = vi.fn(async () => []);
    renderCommit(
      fakeClient({
        getFileSides,
        getCommitDiff,
        getCommitFiles: vi.fn(async () => ["logo.png"]),
        getCommitMessage: async () => "m",
        getCommitSignature: async () => ({ status: "unsigned", format: null, signer: null, key: null, detail: null }),
      }),
      "abc123",
    );
    expect(await screen.findByRole("img", { name: "Before: logo.png" })).toBeInTheDocument();
    expect(getFileSides).toHaveBeenCalledWith(TEST_REPO_PATH, "logo.png", { kind: "Commit", commitId: "abc123", oldPath: null });
    expect(getCommitDiff).not.toHaveBeenCalled();
  });

  it("previews an image in a stash, against the stash's base", async () => {
    const getFileSides = vi.fn(async () => ({ old: pngSide("Zm9v"), new: pngSide("YmFy") }));
    renderCommit(
      fakeClient({
        getFileSides,
        getStashFiles: vi.fn(async () => ({ tracked: ["logo.png"], untracked: [], untrackedCommitId: null })),
        getCommitDiff: vi.fn(async () => []),
        getBlame: async () => [],
      }),
      "s0",
      {
        stashes: [{ index: 0, message: "On main: wip", commitId: "s0" }],
        stashActions: {
          onApply: vi.fn(),
          onPop: vi.fn(),
          onDrop: vi.fn(),
          restoreIndex: true,
          onSetRestoreIndex: vi.fn(),
          disabledReason: null,
        },
      },
    );
    expect(await screen.findByRole("img", { name: "After: logo.png" })).toBeInTheDocument();
    expect(getFileSides).toHaveBeenCalledWith(TEST_REPO_PATH, "logo.png", { kind: "Commit", commitId: "s0", oldPath: null });
  });
});

describe("DiffPane commit message settings", () => {
  it("hands the Preferences counter limit and ruler to the commit box", () => {
    rtlRender(
      <CommitMessageSettingsContext.Provider
        value={{ settings: { summaryLimit: 60, bodyRuler: 80, showRuler: true }, update: vi.fn(), error: null, dismissError: vi.fn() }}
      >
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={fakeClient({})}
          selectedRow="uncommitted"
          status={[]}
          mergeMessage={null}
          rebaseProgress={null}
          {...noopHandlers}
        />
      </CommitMessageSettingsContext.Provider>,
    );
    expect(screen.getByLabelText("Summary length")).toHaveTextContent("Summary 0/60");
    expect(screen.getByRole("textbox", { name: "Commit message" }).style.getPropertyValue("--ruler-column")).toBe("80");
  });
});

describe("DiffPane co-authors and amend author", () => {
  const headInfo = { headId: "h1", detachedAt: null, headPushed: false, operation: null, squashMessage: null, commitHooks: [], sequence: null };
  const head = {
    id: "h1",
    shortId: "h1",
    summary: "head",
    authorName: "Me",
    authorEmail: "me@x.org",
    timestamp: 0,
    parentIds: [],
    branchRefs: [],
    remoteBranchRefs: [],
  };

  it("prefills the amend Author from the graph and amends with the options", async () => {
    const onAmendCommit = vi.fn(async () => true);
    renderUncommitted(fakeClient({ getCommitMessage: async () => "head message" }), [], {
      amendMode: true,
      headInfo,
      commits: [head],
      onAmendCommit,
    });
    const author = await screen.findByRole("textbox", { name: "Author" });
    expect(author).toHaveValue("Me <me@x.org>");
    await waitFor(() => expect(screen.getByLabelText("Commit message")).toHaveValue("head message"));
    fireEvent.click(screen.getByRole("button", { name: "Amend" }));
    expect(onAmendCommit).toHaveBeenCalledWith("head message", { author: null, trailers: [] });
  });

  it("leaves the Author empty when HEAD is not in the graph", async () => {
    renderUncommitted(fakeClient({ getCommitMessage: async () => "head message" }), [], {
      amendMode: true,
      headInfo,
      commits: [],
    });
    expect(await screen.findByRole("textbox", { name: "Author" })).toHaveValue("");
  });

  it("offers the repository's recent authors and commits them as trailers", async () => {
    const listRecentAuthors = vi.fn(async () => [{ name: "Ada", email: "ada@x.org" }]);
    const onCommit = vi.fn(async () => true);
    renderUncommitted(fakeClient({ listRecentAuthors }), [{ path: "a.txt", staged: true, kind: "Modified" }], {
      onCommit,
    });
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "Fix" } });
    fireEvent.click(screen.getByRole("button", { name: "Add co-author" }));
    fireEvent.click(await screen.findByRole("option", { name: "Ada <ada@x.org>" }));
    fireEvent.click(screen.getByRole("button", { name: "Commit" }));
    expect(listRecentAuthors).toHaveBeenCalledWith(TEST_REPO_PATH);
    expect(onCommit).toHaveBeenCalledWith("Fix", [{ key: "Co-authored-by", value: "Ada <ada@x.org>" }]);
  });
});
