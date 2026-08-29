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
    getWorkingDiff: async () => [],
    getCommitDiff: async () => [],
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

const noopHandlers = {
  onStageFile: vi.fn(),
  onUnstageFile: vi.fn(),
  onStageAllFiles: vi.fn(),
  onUnstageAllFiles: vi.fn(),
  onStageHunk: vi.fn(),
  onUnstageHunk: vi.fn(),
  onDiscardHunk: vi.fn(),
  onCommit: vi.fn(),
  onSaveStash: vi.fn(),
  onSelectRow: vi.fn(),
  onResolveConflict: vi.fn(),
  onResolveAddDeleteConflict: vi.fn(),
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
      expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "a.txt", false);
      expect(getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "b.txt", true);
    });

    it("shows Stage Hunk (not Unstage Hunk) and Discard Hunk for an unstaged file's diff", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      // Only a.txt (unstaged) gets hunks — b.txt (staged) stays empty, so the assertions below
      // are unambiguous now that both sections render at once.
      const getWorkingDiff = vi.fn(async (_repoPath: string, path: string) => (path === "a.txt" ? hunks : []));
      renderUncommitted(fakeClient({ getWorkingDiff }), status);

      expect(await screen.findByText("Stage Hunk")).toBeInTheDocument();
      expect(screen.queryByText("Unstage Hunk")).not.toBeInTheDocument();
      expect(screen.getByText("Discard Hunk")).toBeInTheDocument();
    });

    it("shows Unstage Hunk (not Stage Hunk) for a staged file's diff", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      // Only b.txt (staged) gets hunks this time — a.txt stays empty.
      const getWorkingDiff = vi.fn(async (_repoPath: string, path: string) => (path === "b.txt" ? hunks : []));
      renderUncommitted(fakeClient({ getWorkingDiff }), status);

      expect(await screen.findByText("Unstage Hunk")).toBeInTheDocument();
      expect(screen.queryByText("Stage Hunk")).not.toBeInTheDocument();
    });

    it("clicking Stage Hunk calls onStageHunk with that section's path and the hunk's start lines", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 3, oldLines: 1, newStart: 4, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      const onStageHunk = vi.fn();
      renderUncommitted(fakeClient({ getWorkingDiff: async () => hunks }), status, { onStageHunk });

      fireEvent.click(await screen.findByText("Stage Hunk"));

      expect(onStageHunk).toHaveBeenCalledWith("a.txt", 3, 4);
    });

    it("clicking the Stage control calls onStageFile with that path", () => {
      const onStageFile = vi.fn();
      renderUncommitted(fakeClient({}), status, { onStageFile });

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

    it("Back to Diff switches that section back to the diff view", async () => {
      const blameLines: BlameLine[] = [
        { lineNumber: 1, content: "hello", commitId: "abc123", shortId: "abc1234", authorName: "Rene", timestamp: 1 },
      ];
      const getBlame = vi.fn(async () => blameLines);
      const getWorkingDiff = vi.fn(async () => []);
      renderUncommitted(fakeClient({ getBlame, getWorkingDiff }), status);

      fireEvent.click(screen.getAllByText("Blame")[0]);
      await screen.findByText("hello");
      fireEvent.click(screen.getByText("Back to Diff"));

      expect(screen.queryByText("Back to Diff")).not.toBeInTheDocument();
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
      await waitFor(() => expect(client.getWorkingDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "shared.txt", false));
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

      await waitFor(() => screen.getByText("Keep Our Version"));
      await waitFor(() => screen.getByText("Save resolution"));

      // Both sections are mounted simultaneously now — one file's add/delete fallback state
      // must not appear on the other's section.
      expect(screen.getAllByText("Keep Our Version")).toHaveLength(1);
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
      expect(getCommitDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "abc123", "src/main.rs");
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
