import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { BlameLine, ConflictSegment, DiffHunk, RepoClient, StatusEntry } from "../ipc/RepoClient";
import { DiffPane } from "./DiffPane";

function unused(): never {
  throw new Error("not used in this test");
}

function fakeClient(overrides: Partial<RepoClient>): RepoClient {
  return {
    pickRepoFolder: unused,
    listRecentRepos: unused,
    openRepo: unused,
    getStatus: unused,
    getCommitGraph: unused,
    listBranches: unused,
    createBranch: unused,
    switchBranch: unused,
    deleteBranch: unused,
    renameBranch: unused,
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
    getWorkingDiff: unused,
    getCommitDiff: unused,
    getCommitFiles: unused,
    stageFile: unused,
    unstageFile: unused,
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

    it("renders a Stage button for unstaged entries and Unstage for staged ones", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      expect(screen.getByText("Stage")).toBeInTheDocument();
      expect(screen.getByText("Unstage")).toBeInTheDocument();
    });

    it("clicking Stage calls onStageFile with that path", () => {
      const client = fakeClient({});
      const onStageFile = vi.fn();

      render(
        <DiffPane
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={onStageFile}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("Stage"));

      expect(onStageFile).toHaveBeenCalledWith("a.txt");
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
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("a.txt (Modified)"));

      expect(await screen.findByText(/changed line/)).toBeInTheDocument();
      expect(getWorkingDiff).toHaveBeenCalledWith("a.txt", false);
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
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("b.txt (New)"));

      expect(await screen.findByText(/staged content/)).toBeInTheDocument();
      expect(getWorkingDiff).toHaveBeenCalledWith("b.txt", true);
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
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
          client={client}
          selectedRow="uncommitted"
          status={refreshedStatus}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
          client={client}
          selectedRow="uncommitted"
          status={unstagedOnly}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      expect(screen.getByText("Commit")).toBeDisabled();
    });

    it("CommitBox is enabled when something is staged", () => {
      const client = fakeClient({});
      const stagedOnly: StatusEntry[] = [{ path: "b.txt", staged: true, kind: "New" }];

      render(
        <DiffPane
          client={client}
          selectedRow="uncommitted"
          status={stagedOnly}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      expect(screen.getByText("Stash")).toBeInTheDocument();
    });

    it("clicking Stash calls onSaveStash", () => {
      const client = fakeClient({});
      const onSaveStash = vi.fn();

      render(
        <DiffPane
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={onSaveStash}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("Stash"));

      expect(onSaveStash).toHaveBeenCalled();
    });

    it("Stash button is disabled when the working tree is clean", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          client={client}
          selectedRow="uncommitted"
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      expect(screen.getByText("Stash")).toBeDisabled();
    });

    it("Stash button is enabled when there are changes", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      expect(screen.getByText("Stash")).not.toBeDisabled();
    });

    it("renders a Blame button per file", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      fireEvent.click(screen.getAllByText("Blame")[0]);

      expect(await screen.findByText("hello")).toBeInTheDocument();
      expect(getBlame).toHaveBeenCalledWith("HEAD", "a.txt");
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
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={onSelectRow}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
          client={client}
          selectedRow="uncommitted"
          status={conflictedStatus}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
          client={client}
          selectedRow="uncommitted"
          status={conflictedStatus}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={"Merge branch 'feature'"}
          onAbortMerge={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("shared.txt (Conflicted)"));
      await waitFor(() => screen.getByText("Save resolution"));

      // Simulate a status refresh after an abort (or after the conflict was already resolved
      // through this same pane): the conflicted entry is gone.
      rerender(
        <DiffPane
          client={client}
          selectedRow="uncommitted"
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      expect(screen.queryByText("Save resolution")).not.toBeInTheDocument();
      // Finding 5: the pane falls back to the real diff view (which fetches), not a
      // permanently blank one — `viewMode` transitioned back to `"diff"`.
      await waitFor(() =>
        expect(client.getWorkingDiff).toHaveBeenCalledWith("shared.txt", false),
      );
    });

    it("switching the selected conflicted file discards stale add/delete fallback state (remounts via key)", async () => {
      const twoConflicts: StatusEntry[] = [
        { path: "binary.dat", staged: false, kind: "Conflicted" },
        { path: "shared.txt", staged: false, kind: "Conflicted" },
      ];
      let resolveSharedHunks: (segments: ConflictSegment[]) => void = () => {};
      const getConflictHunks = vi.fn((path: string): Promise<ConflictSegment[]> => {
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
          client={client}
          selectedRow="uncommitted"
          status={twoConflicts}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
          client={client}
          selectedRow="uncommitted"
          status={mixedStatus}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      const commitButton = screen.getByText("Commit").closest("button");
      expect(commitButton).toBeDisabled();
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
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      expect(await screen.findByText("src/main.rs")).toBeInTheDocument();

      fireEvent.click(screen.getByText("src/main.rs"));

      expect(await screen.findByText(/fn main/)).toBeInTheDocument();
      expect(getCommitDiff).toHaveBeenCalledWith("abc123", "src/main.rs");
    });

    it("no CommitBox or stage/unstage buttons render", async () => {
      const client = fakeClient({ getCommitFiles, getCommitDiff });

      render(
        <DiffPane
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      expect(await screen.findByText("src/main.rs")).toBeInTheDocument();

      expect(screen.queryByText("Stash")).toBeNull();
    });

    it("renders a Blame button per file", async () => {
      const client = fakeClient({ getCommitFiles, getCommitDiff });

      render(
        <DiffPane
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
        />,
      );

      await screen.findByText("src/main.rs");
      fireEvent.click(screen.getByText("Blame"));

      expect(await screen.findByText("fn main() {}")).toBeInTheDocument();
      expect(getBlame).toHaveBeenCalledWith("abc123", "src/main.rs");
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
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={onSelectRow}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
          client={client}
          selectedRow={{ commitId: "abc123" }}
          status={[]}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
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
