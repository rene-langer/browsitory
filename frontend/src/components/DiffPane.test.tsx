import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DiffHunk, RepoClient, StatusEntry } from "../ipc/RepoClient";
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
    getLog: unused,
    listBranches: unused,
    createBranch: unused,
    switchBranch: unused,
    deleteBranch: unused,
    renameBranch: unused,
    listStashes: unused,
    saveStash: unused,
    applyStash: unused,
    dropStash: unused,
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
        />,
      );

      fireEvent.click(screen.getByText("Stash"));

      expect(onSaveStash).toHaveBeenCalled();
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
        />,
      );

      expect(await screen.findByText("src/main.rs")).toBeInTheDocument();

      expect(screen.queryByText("Stash")).toBeNull();
    });
  });
});
