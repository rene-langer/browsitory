import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";
import type { ChangedFile, GraphCommit, RepoClient } from "../ipc/RepoClient";
import type { CompareState } from "../state/useCompare";
import type { ExternalToolsApi } from "../state/useExternalTools";
import { CompareDiffPane } from "./CompareDiffPane";

const A = "a".repeat(40);
const B = "b".repeat(40);
const commit = (id: string, shortId: string, summary: string): GraphCommit => ({
  id,
  shortId,
  summary,
  authorName: "Rene",
  authorEmail: "rene@example.com",
  timestamp: 1,
  parentIds: [],
  branchRefs: [],
  remoteBranchRefs: [],
  tags: [],
});
const commits = [commit(B, "bbbbbbb", "newer"), commit(A, "aaaaaaa", "older")];
const twoCommits: CompareState = { target: { kind: "commits", fromId: A, toId: B }, range: { fromId: A, toId: B }, error: null };

function client(overrides: Partial<RepoClient> = {}): RepoClient {
  return {
    getRangeFiles: vi.fn(async (): Promise<ChangedFile[]> => []),
    getRangeDiff: vi.fn(async () => []),
    getWorkingTreeCompareFiles: vi.fn(async (): Promise<ChangedFile[]> => []),
    getWorkingTreeCompareDiff: vi.fn(async () => []),
    getBlame: vi.fn(async () => []),
    ...overrides,
  } as unknown as RepoClient;
}

function renderPane(props: Partial<ComponentProps<typeof CompareDiffPane>> = {}) {
  const all: ComponentProps<typeof CompareDiffPane> = {
    repoPath: "/repo",
    client: client(),
    compare: twoCommits,
    commits,
    refreshGeneration: 0,
    onSwap: vi.fn(),
    onClose: vi.fn(),
    onRetry: vi.fn(),
    onSelectRow: vi.fn(),
    ...props,
  };
  return { ...render(<CompareDiffPane {...all} />), props: all };
}

describe("CompareDiffPane", () => {
  it("names both commits and lists the range's files, renames included", async () => {
    const getRangeDiff = vi.fn(async () => []);
    renderPane({
      client: client({
        getRangeFiles: vi.fn(async () => [
          { path: "a.txt", oldPath: null },
          { path: "new.txt", oldPath: "old.txt" },
        ]),
        getRangeDiff,
      }),
    });
    const header = screen.getByRole("region", { name: "Comparison" });
    expect(within(header).getByRole("heading")).toHaveTextContent("Comparing aaaaaaa..bbbbbbb");
    expect(header).toHaveTextContent("aaaaaaa older");
    expect(header).toHaveTextContent("bbbbbbb newer");
    expect(await screen.findByText("2 files changed")).toBeInTheDocument();
    expect(screen.getByText("Renamed from old.txt")).toBeInTheDocument();
    await waitFor(() =>
      expect(getRangeDiff).toHaveBeenCalledWith("/repo", A, B, "new.txt", "old.txt", { ignoreWhitespace: false, contextLines: 3 }),
    );
  });

  it("Swap and × call back; only two-commit compares can swap", () => {
    const { props, unmount } = renderPane();
    fireEvent.click(screen.getByRole("button", { name: "Swap sides" }));
    expect(props.onSwap).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Close compare" }));
    expect(props.onClose).toHaveBeenCalled();
    unmount();

    renderPane({ compare: { target: { kind: "workingTree", commitId: A }, range: { fromId: A, toId: null }, error: null } });
    expect(screen.queryByRole("button", { name: "Swap sides" })).toBeNull();
  });

  it("says No differences. for an empty range", async () => {
    renderPane();
    expect(await screen.findByText("No differences.")).toBeInTheDocument();
  });

  it("starts every section collapsed above the expand limit", async () => {
    const files = Array.from({ length: 51 }, (_, i) => ({ path: `f${i}.txt`, oldPath: null }));
    const getRangeDiff = vi.fn(async () => []);
    renderPane({ client: client({ getRangeFiles: vi.fn(async () => files), getRangeDiff }) });
    expect(await screen.findByText("51 files changed")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /^Expand f\d+\.txt$/ })).toHaveLength(51);
    expect(getRangeDiff).not.toHaveBeenCalled();
  });

  it("a working-tree compare reloads on refresh and blames at HEAD", async () => {
    const getWorkingTreeCompareFiles = vi.fn(async () => [{ path: "a.txt", oldPath: null }]);
    const getBlame = vi.fn(async () => []);
    const c = client({ getWorkingTreeCompareFiles, getBlame });
    const compare: CompareState = { target: { kind: "workingTree", commitId: A }, range: { fromId: A, toId: null }, error: null };
    const { rerender, props } = renderPane({ client: c, compare });
    expect(screen.getByRole("heading")).toHaveTextContent("Comparing aaaaaaa with the working tree");
    expect(screen.getByRole("region", { name: "Comparison" })).toHaveTextContent("Working tree");
    await screen.findByText("1 file changed");

    rerender(<CompareDiffPane {...props} client={c} compare={compare} refreshGeneration={1} />);
    await waitFor(() => expect(getWorkingTreeCompareFiles).toHaveBeenCalledTimes(2));

    fireEvent.click(screen.getByRole("button", { name: "Blame" }));
    await waitFor(() => expect(getBlame).toHaveBeenCalledWith("/repo", "HEAD", "a.txt"));
  });

  it("labels a branch compare and waits for its range", () => {
    const resolving: CompareState = { target: { kind: "branch", refName: "refs/heads/feature", label: "feature" }, range: null, error: null };
    const { unmount } = renderPane({ compare: resolving });
    expect(screen.getByRole("heading")).toHaveTextContent("Changes on feature since it diverged");
    expect(screen.getByText("Finding where feature diverged…")).toBeInTheDocument();
    unmount();

    renderPane({ compare: { ...resolving, range: { fromId: A, toId: B } } });
    const header = screen.getByRole("region", { name: "Comparison" });
    expect(header).toHaveTextContent("aaaaaaa (merge base)");
    expect(header).toHaveTextContent("bbbbbbb feature");
  });

  it("a resolve error offers Retry, and Dismiss closes the compare", () => {
    const { props } = renderPane({
      compare: {
        target: { kind: "branch", refName: "refs/heads/orphan", label: "orphan" },
        range: null,
        error: { message: "orphan and HEAD have no common ancestor." },
      },
    });
    expect(screen.getByText("orphan and HEAD have no common ancestor.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(props.onRetry).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Dismiss error" }));
    expect(props.onClose).toHaveBeenCalled();
  });

  it("a file-list error retries the file list", async () => {
    const getRangeFiles = vi
      .fn<() => Promise<ChangedFile[]>>()
      .mockRejectedValueOnce("failed to compute diff: boom")
      .mockResolvedValueOnce([{ path: "a.txt", oldPath: null }]);
    renderPane({ client: client({ getRangeFiles }) });
    fireEvent.click(await screen.findByRole("button", { name: "Retry" }));
    expect(await screen.findByText("1 file changed")).toBeInTheDocument();
  });

  it("drops the old file list when the range changes, and never diffs an old path in the new range", async () => {
    const C = "c".repeat(40);
    const getRangeFiles = vi.fn(async (_repo: string, from: string): Promise<ChangedFile[]> =>
      from === A ? [{ path: "old-range.txt", oldPath: null }] : new Promise<ChangedFile[]>(() => {}),
    );
    const getRangeDiff = vi.fn(async () => []);
    const c = client({ getRangeFiles, getRangeDiff });
    const { rerender, props } = renderPane({ client: c });
    expect(await screen.findByText("old-range.txt")).toBeInTheDocument();

    const next: CompareState = { target: { kind: "commits", fromId: C, toId: B }, range: { fromId: C, toId: B }, error: null };
    rerender(<CompareDiffPane {...props} client={c} compare={next} />);
    expect(screen.queryByText("old-range.txt")).toBeNull();
    await waitFor(() => expect(getRangeFiles).toHaveBeenCalledWith("/repo", C, B));
    expect(getRangeDiff).not.toHaveBeenCalledWith("/repo", C, B, "old-range.txt", null, expect.anything());
  });
});

describe("CompareDiffPane external tools", () => {
  function tools(): ExternalToolsApi {
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
    };
  }
  const pick = (path: string, label: string) => {
    fireEvent.click(screen.getByRole("button", { name: `Actions for ${path}` }));
    fireEvent.click(screen.getByRole("menuitem", { name: label }));
  };

  it("compare file sections open the range in the diff tool", async () => {
    const externalTools = tools();
    renderPane({
      externalTools,
      client: client({ getRangeFiles: vi.fn(async () => [{ path: "new.txt", oldPath: "old.txt" }]) }),
    });
    await screen.findByText("1 file changed");

    pick("new.txt", "Open in diff tool");
    expect(externalTools.openDiffTool).toHaveBeenCalledWith("new.txt", { kind: "Range", fromId: A, toId: B, oldPath: "old.txt" });
    pick("new.txt", "Open in editor");
    expect(externalTools.openInEditor).toHaveBeenCalledWith("new.txt", null);
  });

  it("a working-tree compare passes a null toId", async () => {
    const externalTools = tools();
    const compare: CompareState = { target: { kind: "workingTree", commitId: A }, range: { fromId: A, toId: null }, error: null };
    renderPane({
      externalTools,
      compare,
      client: client({ getWorkingTreeCompareFiles: vi.fn(async () => [{ path: "a.txt", oldPath: null }]) }),
    });
    await screen.findByText("1 file changed");

    pick("a.txt", "Open in diff tool");
    expect(externalTools.openDiffTool).toHaveBeenCalledWith("a.txt", { kind: "Range", fromId: A, toId: null, oldPath: null });
  });

  it("has no actions menu without externalTools", async () => {
    renderPane({ client: client({ getRangeFiles: vi.fn(async () => [{ path: "a.txt", oldPath: null }]) }) });
    await screen.findByText("1 file changed");
    expect(screen.queryByRole("button", { name: "Actions for a.txt" })).toBeNull();
  });
});
