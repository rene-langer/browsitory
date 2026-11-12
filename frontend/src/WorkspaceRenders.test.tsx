import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { BranchInfo, GraphCommit, RepoClient } from "./ipc/RepoClient";
import { fakeRepoClient, graphPage } from "./test/fakeRepoClient";

// Which views render on an update that doesn't concern them (#844). Each counter stands for one
// view: `BranchTreeView` renders once per `BranchTree` render, `CommitGraph` calls
// `useCommitGraphLayout` once per render (with no commits of its own: the workspace passes the
// shared layout, and the workspace's own call has the commits), and the diff pane renders one of
// its panes.
const counts = vi.hoisted(() => ({ branchTreeView: 0, buildBranchTree: 0, commitGraph: 0, diffPane: 0, worktreePanel: 0 }));
function resetCounts() {
  counts.worktreePanel = 0;
  counts.branchTreeView = 0;
  counts.buildBranchTree = 0;
  counts.commitGraph = 0;
  counts.diffPane = 0;
}
vi.mock("./components/BranchTreeView", async (importOriginal) => {
  const original = await importOriginal<typeof import("./components/BranchTreeView")>();
  return {
    ...original,
    BranchTreeView: (props: Parameters<typeof original.BranchTreeView>[0]) => {
      counts.branchTreeView += 1;
      return <original.BranchTreeView {...props} />;
    },
  };
});
vi.mock("./lib/branchTree", async (importOriginal) => {
  const original = await importOriginal<typeof import("./lib/branchTree")>();
  return {
    ...original,
    buildBranchTree: ((items) => {
      counts.buildBranchTree += 1;
      return original.buildBranchTree(items);
    }) as typeof original.buildBranchTree,
  };
});
// `WorktreePanel` asks for its Prune action once per render (the palette asks too, but only
// while it builds its commands).
vi.mock("./lib/actions/worktreeActions", async (importOriginal) => {
  const original = await importOriginal<typeof import("./lib/actions/worktreeActions")>();
  return {
    ...original,
    pruneWorktreesAction: ((...args) => {
      counts.worktreePanel += 1;
      return original.pruneWorktreesAction(...args);
    }) as typeof original.pruneWorktreesAction,
  };
});
vi.mock("./lib/useCommitGraphLayout", async (importOriginal) => {
  const original = await importOriginal<typeof import("./lib/useCommitGraphLayout")>();
  return {
    ...original,
    useCommitGraphLayout: ((commits, expanded) => {
      if (commits.length === 0) counts.commitGraph += 1;
      return original.useCommitGraphLayout(commits, expanded);
    }) as typeof original.useCommitGraphLayout,
  };
});
vi.mock("./components/UncommittedDiffPane", async (importOriginal) => {
  const original = await importOriginal<typeof import("./components/UncommittedDiffPane")>();
  return {
    ...original,
    UncommittedDiffPane: (props: Parameters<typeof original.UncommittedDiffPane>[0]) => {
      counts.diffPane += 1;
      return <original.UncommittedDiffPane {...props} />;
    },
  };
});
vi.mock("./components/CommitDiffPane", async (importOriginal) => {
  const original = await importOriginal<typeof import("./components/CommitDiffPane")>();
  return {
    ...original,
    CommitDiffPane: (props: Parameters<typeof original.CommitDiffPane>[0]) => {
      counts.diffPane += 1;
      return <original.CommitDiffPane {...props} />;
    },
  };
});

vi.setConfig({ testTimeout: 30_000 });
// The tree's folders remember being open or closed; each test starts from the defaults.
beforeEach(() => localStorage.clear());

const id = (i: number) => i.toString(16).padStart(40, "0");

function commits(count: number): GraphCommit[] {
  return Array.from({ length: count }, (_, i) => ({
    id: id(i),
    shortId: id(i).slice(0, 7),
    summary: `Commit ${i}`,
    authorName: "A",
    authorEmail: "a@example.com",
    timestamp: 1_700_000_000 - i * 60,
    parentIds: i + 1 < count ? [id(i + 1)] : [],
    branchRefs: i === 0 ? ["main"] : [],
    remoteBranchRefs: [],
    tags: [],
  }));
}

function client(): RepoClient {
  const branches: BranchInfo[] = [{ name: "main", isCurrent: true, upstream: null, ahead: null, behind: null }];
  for (let i = 0; i < 300; i++) {
    branches.push({ name: `team-${i % 10}/topic-${i}`, isCurrent: false, upstream: null, ahead: null, behind: null });
  }
  const remote = Array.from({ length: 300 }, (_, i) => `pr-${i % 10}/head-${i}`);
  const history = commits(40);
  let selection: string[] | null = null;
  return fakeRepoClient(
    {
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => graphPage(history),
      listBranches: async () => branches,
      listStashes: async () => [],
      listRemotes: async () => [
        { name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null, authMode: null, authUsername: null },
      ],
      listRemoteBranches: async () => remote,
      setGraphBranchSelection: async (_path, local) => {
        selection = local;
      },
      clearGraphBranchSelection: async () => {
        selection = null;
      },
      getGraphBranchSelection: async () => selection,
    },
    "empty",
  );
}

async function settle() {
  await act(async () => new Promise((resolve) => setTimeout(resolve, 50)));
}

async function openWorkspace() {
  const view = render(<App client={client()} />);
  await screen.findByRole("tab", { name: "a" });
  await waitFor(() => expect(view.container.querySelector(`[data-commit-id="${id(0)}"]`)).not.toBeNull());
  const origin = await screen.findByRole("button", { name: "origin" });
  if (origin.getAttribute("aria-expanded") !== "true") fireEvent.click(origin);
  await screen.findByText("head-0");
  await screen.findByText("topic-0");
  await settle();
  return view;
}

describe("workspace renders (#844)", () => {
  it("opening and closing the command palette renders neither the sidebar, the graph nor the diff", async () => {
    await openWorkspace();
    resetCounts();
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const input = await screen.findByRole("combobox", { name: "Command palette" });
    fireEvent(input.closest("dialog")!, new Event("close"));
    await waitFor(() => expect(screen.queryByRole("combobox", { name: "Command palette" })).toBeNull());
    await settle();
    expect(counts).toMatchObject({ branchTreeView: 0, buildBranchTree: 0, commitGraph: 0, diffPane: 0 });
  });

  it("hiding a branch from the graph renders the tree without building it again", async () => {
    await openWorkspace();
    resetCounts();
    fireEvent.click(screen.getByRole("button", { name: "Show team-1/topic-1 in graph" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Show team-1/topic-1 in graph" })).toHaveAttribute("aria-pressed", "false"),
    );
    await settle();
    expect(counts.branchTreeView).toBeGreaterThan(0);
    expect(counts.buildBranchTree).toBe(0);
  });

  it("selecting a commit renders neither the branch tree nor the worktree panel (its branch list)", async () => {
    const view = await openWorkspace();
    resetCounts();
    fireEvent.click(view.container.querySelector(`[data-commit-id="${id(3)}"]`)!);
    await settle();
    expect(counts.commitGraph).toBeGreaterThan(0);
    expect(counts.branchTreeView).toBe(0);
    expect(counts.worktreePanel).toBe(0);
  });

  it("a branch list that changes still shows in the tree", async () => {
    // Guards the memoization: new data must still reach the memoized views.
    await openWorkspace();
    expect(screen.getByRole("button", { name: "Show team-1/topic-1 in graph" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Show team-1/topic-1 in graph" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Show team-1/topic-1 in graph" })).toHaveAttribute("aria-pressed", "false"),
    );
    expect(screen.getByRole("button", { name: "Show team-2/topic-2 in graph" })).toHaveAttribute("aria-pressed", "true");
  });
});
