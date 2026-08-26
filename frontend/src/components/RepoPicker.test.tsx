import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { RepoPicker } from "./RepoPicker";

function unimplemented(): never {
  throw new Error("not implemented in this fake");
}

function fakeClient(overrides: Partial<RepoClient>): RepoClient {
  return {
    pickRepoFolder: async () => unimplemented(),
    listRecentRepos: async () => unimplemented(),
    scanReposInRoot: async () => unimplemented(),
    listWorkspaces: async () => unimplemented(),
    saveWorkspace: async () => unimplemented(),
    updateWorkspace: async () => unimplemented(),
    deleteWorkspace: async () => unimplemented(),
    openRepo: async () => {},
    closeRepo: async () => unimplemented(),
    listOpenRepos: async () => unimplemented(),
    persistOpenRepos: async () => unimplemented(),
    getStatus: async () => unimplemented(),
    getCommitGraph: async () => unimplemented(),
    listBranches: async () => unimplemented(),
    createBranch: async () => unimplemented(),
    switchBranch: async () => unimplemented(),
    deleteBranch: async () => unimplemented(),
    renameBranch: async () => unimplemented(),
    listWorktrees: async () => unimplemented(),
    createWorktree: async () => unimplemented(),
    removeWorktree: async () => unimplemented(),
    pruneWorktrees: async () => unimplemented(),
  listSubmodules: async () => [],
  initSubmodule: async () => unimplemented(),
  updateSubmodule: async () => unimplemented(),
    listReflogRefs: async () => [],
    getReflog: async () => [],
    restoreReflogEntry: async () => unimplemented(),
    listRemotes: async () => unimplemented(),
    listRemoteBranches: async () => unimplemented(),
    getCurrentUpstream: async () => null,
    getRemoteUpstreams: async () => [],
    addRemote: async () => unimplemented(),
    renameRemote: async () => unimplemented(),
    updateRemoteUrls: async () => unimplemented(),
    removeRemote: async () => unimplemented(),
    saveHttpsCredential: async () => unimplemented(),
    forgetHttpsCredential: async () => unimplemented(),
    setRemoteAuthMode: async () => unimplemented(),
    setCurrentUpstream: async () => unimplemented(),
    clearCurrentUpstream: async () => unimplemented(),
    listTags: async () => [],
    createTag: async () => unimplemented(),
    deleteTag: async () => unimplemented(),
    fetchRemote: async () => unimplemented(),
    pushCurrentBranch: async () => unimplemented(),
    pushTags: async () => unimplemented(),
    pullCurrentUpstream: async () => unimplemented(),
    subscribeTransferProgress: () => () => {},
    listStashes: async () => unimplemented(),
    saveStash: async () => unimplemented(),
    applyStash: async () => unimplemented(),
    dropStash: async () => unimplemented(),
    getBlame: async () => unimplemented(),
    mergeBranch: async () => unimplemented(),
    getConflictHunks: async () => unimplemented(),
    resolveConflict: async () => unimplemented(),
    abortMerge: async () => unimplemented(),
    getMergeMessage: async () => null,
    resolveAddDeleteConflict: async () => unimplemented(),
    commitsSince: async () => unimplemented(),
    startRebase: async () => unimplemented(),
    rebaseContinue: async () => unimplemented(),
    abortRebase: async () => unimplemented(),
    getRebaseProgress: async () => null,
    detectForgeRepository: async () => [],
    saveForgeToken: async () => unimplemented(),
    forgetForgeToken: async () => unimplemented(),
    listPullRequests: async () => unimplemented(),
    createPullRequest: async () => unimplemented(),
    openExternalUrl: async () => unimplemented(),
    getGraphBranchSelection: async () => null,
    setGraphBranchSelection: async () => unimplemented(),
    getWorkingDiff: async () => unimplemented(),
    getCommitDiff: async () => unimplemented(),
    getCommitFiles: async () => unimplemented(),
    stageFile: async () => unimplemented(),
    unstageFile: async () => unimplemented(),
    stageHunk: async () => unimplemented(),
    unstageHunk: async () => unimplemented(),
    discardHunk: async () => unimplemented(),
    commit: async () => unimplemented(),
    ...overrides,
  };
}

function workspaceProps() {
  return {
    onOpenWorkspace: vi.fn(),
    workspaces: [],
    workspacesLoading: false,
    workspacesError: null,
    onCreateWorkspace: vi.fn(),
    onEditWorkspace: vi.fn(),
    onDeleteWorkspace: vi.fn(),
  };
}

describe("RepoPicker", () => {
  it("renders each recent repo and opens it on click", async () => {
    const client = fakeClient({
      listRecentRepos: async () => ["/repo/a", "/repo/b"],
    });
    const onOpenRepo = vi.fn();

    render(<RepoPicker client={client} onOpenRepo={onOpenRepo} {...workspaceProps()} />);

    expect(await screen.findByText("/repo/a")).toBeInTheDocument();
    expect(await screen.findByText("/repo/b")).toBeInTheDocument();

    fireEvent.click(screen.getByText("/repo/a"));

    expect(onOpenRepo).toHaveBeenCalledTimes(1);
    expect(onOpenRepo).toHaveBeenCalledWith("/repo/a");
  });

  it("shows a message when there are no recent repos", async () => {
    const client = fakeClient({
      listRecentRepos: async () => [],
    });

    render(<RepoPicker client={client} onOpenRepo={vi.fn()} {...workspaceProps()} />);

    expect(await screen.findByText("No recent repositories")).toBeInTheDocument();
  });

  it("Open Folder button opens the picked path", async () => {
    const client = fakeClient({
      listRecentRepos: async () => [],
      pickRepoFolder: async () => "/picked/repo",
    });
    const onOpenRepo = vi.fn();

    render(<RepoPicker client={client} onOpenRepo={onOpenRepo} {...workspaceProps()} />);

    fireEvent.click(screen.getByText("Open Folder"));

    await waitFor(() => {
      expect(onOpenRepo).toHaveBeenCalledWith("/picked/repo");
    });
  });

  it("Open Folder button does nothing when the dialog is cancelled", async () => {
    const client = fakeClient({
      listRecentRepos: async () => [],
      pickRepoFolder: async () => null,
    });
    const onOpenRepo = vi.fn();

    render(<RepoPicker client={client} onOpenRepo={onOpenRepo} {...workspaceProps()} />);

    fireEvent.click(screen.getByText("Open Folder"));

    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(onOpenRepo).not.toHaveBeenCalled();
  });

  it("renders the error instead of the recent list when listRecentRepos rejects", async () => {
    const client = fakeClient({
      listRecentRepos: async () => {
        throw new Error("config unreadable");
      },
    });

    render(<RepoPicker client={client} onOpenRepo={vi.fn()} {...workspaceProps()} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("config unreadable");
  });
});

describe("RepoPicker workspaces", () => {
  const workspace = {
    id: "ws-1",
    name: "Services",
    rootPath: "/projects",
    memberPaths: ["/projects/a", "/projects/b"],
  };

  it("renders each saved workspace by name, with its root path as a tooltip", () => {
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[workspace]}
        workspacesLoading={false}
        workspacesError={null}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );

    expect(screen.getByText("Services")).toBeInTheDocument();
    expect(screen.getByTitle("/projects")).toBeInTheDocument();
  });

  it("Open All calls onOpenWorkspace with the workspace", () => {
    const onOpenWorkspace = vi.fn();
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={onOpenWorkspace}
        workspaces={[workspace]}
        workspacesLoading={false}
        workspacesError={null}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Open All" }));

    expect(onOpenWorkspace).toHaveBeenCalledWith(workspace);
  });

  it("Delete asks for confirmation before calling onDeleteWorkspace", async () => {
    const onDeleteWorkspace = vi.fn().mockResolvedValue(undefined);
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[workspace]}
        workspacesLoading={false}
        workspacesError={null}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={onDeleteWorkspace}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Delete Services" }));
    expect(onDeleteWorkspace).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Delete workspace" }));
    await waitFor(() => expect(onDeleteWorkspace).toHaveBeenCalledWith("ws-1"));
  });

  it("Open Workspace Root shows the WorkspaceEditor in create mode", () => {
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[]}
        workspacesLoading={false}
        workspacesError={null}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Open Workspace Root"));

    expect(screen.getByText("New Workspace")).toBeInTheDocument();
  });

  it("opens the selected members with the saved workspace id after creation", async () => {
    const client = fakeClient({
      listRecentRepos: async () => [],
      pickRepoFolder: async () => "/projects/root",
      scanReposInRoot: async () => ["/projects/root/a", "/projects/root/b"],
    });
    const onCreateWorkspace = vi.fn().mockResolvedValue("ws-saved");
    const onOpenWorkspace = vi.fn();
    render(
      <RepoPicker
        client={client}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={onOpenWorkspace}
        workspaces={[]}
        workspacesLoading={false}
        workspacesError={null}
        onCreateWorkspace={onCreateWorkspace}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Open Workspace Root"));
    fireEvent.click(screen.getByText("Choose Root Folder"));
    await waitFor(() => expect(screen.getAllByRole("checkbox")).toHaveLength(2));
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() =>
      expect(onOpenWorkspace).toHaveBeenCalledWith({
        id: "ws-saved",
        name: "root",
        rootPath: "/projects/root",
        memberPaths: ["/projects/root/a", "/projects/root/b"],
      }),
    );
    expect(screen.queryByText("New Workspace")).not.toBeInTheDocument();
  });
});
