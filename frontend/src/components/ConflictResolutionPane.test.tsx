import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ConflictSegment, RepoClient } from "../ipc/RepoClient";
import { ConflictResolutionPane } from "./ConflictResolutionPane";

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
    getWorkingDiff: unused,
    getCommitDiff: unused,
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
    mergeBranch: unused,
    getConflictHunks: unused,
    resolveConflict: unused,
    abortMerge: unused,
    getMergeMessage: unused,
    getHeadInfo: unused,
    amendCommit: unused,
    getUndoState: unused,
    getCommitSignature: unused,
    undo: unused,
    redo: unused,
    cherryPick: unused,
    revertCommit: unused,
    resetToCommit: unused,
    checkoutCommit: unused,
    forcePushCurrentBranch: unused,
    resolveAddDeleteConflict: unused,
    commitsSince: unused,
    startRebase: unused,
    rebaseContinue: unused,
    abortRebase: unused,
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
    logFrontendError: unused,
    ...overrides,
  };
}

// Segment content carries embedded trailing newlines, mirroring what
// `parse_conflict_markers` (crates/git-core/src/merge.rs) actually produces via
// `split_inclusive('\n')` — each piece keeps its own line terminator.
const segments: ConflictSegment[] = [
  { kind: "Clean", content: "line one\n" },
  { kind: "Conflict", ours: "main two\n", theirs: "feature two\n" },
  { kind: "Clean", content: "line three\n" },
];

describe("ConflictResolutionPane", () => {
  it("renders clean segments as text and conflict segments with both sides", async () => {
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={vi.fn()} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText(/line one/));
    expect(screen.getByText(/main two/)).toBeInTheDocument();
    expect(screen.getByText(/feature two/)).toBeInTheDocument();
    expect(screen.getByText(/line three/)).toBeInTheDocument();
  });

  it("Save resolution defaults to ours for every conflict and calls onResolve with the joined text", async () => {
    const onResolve = vi.fn();
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText(/line one/));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nmain two\nline three\n");
  });

  it("Accept theirs changes that conflict's contribution to the saved text", async () => {
    const onResolve = vi.fn();
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText("Accept theirs"));
    fireEvent.click(screen.getByText("Accept theirs"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nfeature two\nline three\n");
  });

  it("Accept both concatenates ours then theirs", async () => {
    const onResolve = vi.fn();
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText("Accept both"));
    fireEvent.click(screen.getByText("Accept both"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith(
      "shared.txt",
      "line one\nmain two\nfeature two\nline three\n",
    );
  });

  it("Save resolution is disabled until the conflict hunks finish loading", async () => {
    const onResolve = vi.fn();
    let resolveHunks: (segments: ConflictSegment[]) => void = () => {};
    const client = fakeClient({
      getConflictHunks: () => new Promise((resolve) => (resolveHunks = resolve)),
    });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    const saveButton = screen.getByText("Save resolution").closest("button");
    expect(saveButton).toBeDisabled();
    fireEvent.click(screen.getByText("Save resolution"));
    expect(onResolve).not.toHaveBeenCalled();

    resolveHunks(segments);
    await waitFor(() => expect(saveButton).not.toBeDisabled());
  });

  it("Accept both does not insert a spurious blank line when one side of a conflict is empty", async () => {
    const onResolve = vi.fn();
    const oneEmptySide: ConflictSegment[] = [
      { kind: "Clean", content: "line one\n" },
      { kind: "Conflict", ours: "line two\n", theirs: "" },
      { kind: "Clean", content: "line three\n" },
    ];
    const client = fakeClient({ getConflictHunks: async () => oneEmptySide });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText("Accept both"));
    fireEvent.click(screen.getByText("Accept both"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nline two\nline three\n");
  });

  it("Accept ours does not insert a spurious blank line when theirs is empty", async () => {
    const onResolve = vi.fn();
    const theirsEmpty: ConflictSegment[] = [
      { kind: "Clean", content: "line one\n" },
      { kind: "Conflict", ours: "line two\n", theirs: "" },
      { kind: "Clean", content: "line three\n" },
    ];
    const client = fakeClient({ getConflictHunks: async () => theirsEmpty });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText("Accept ours"));
    fireEvent.click(screen.getByText("Accept ours"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nline two\nline three\n");
  });

  it("Accept theirs does not insert a spurious blank line when ours is empty", async () => {
    const onResolve = vi.fn();
    const oursEmpty: ConflictSegment[] = [
      { kind: "Clean", content: "line one\n" },
      { kind: "Conflict", ours: "", theirs: "line two\n" },
      { kind: "Clean", content: "line three\n" },
    ];
    const client = fakeClient({ getConflictHunks: async () => oursEmpty });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText("Accept theirs"));
    fireEvent.click(screen.getByText("Accept theirs"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nline two\nline three\n");
  });

  it("shows a keep-ours/keep-theirs/delete fallback for an add/delete conflict", async () => {
    const client = fakeClient({
      getConflictHunks: async () => {
        throw new Error("'binary.dat' is an add/delete conflict, not a text conflict");
      },
    });

    render(
      <ConflictResolutionPane
        repoPath={TEST_REPO_PATH}
        client={client}
        path="binary.dat"
        onResolve={vi.fn()}
        onResolveAddDelete={vi.fn()}
      />,
    );

    await waitFor(() => screen.getByText("Keep our version"));
    expect(screen.getByText("Keep their version")).toBeInTheDocument();
    expect(screen.getByText("Delete file")).toBeInTheDocument();
  });

  it("clicking Keep their version calls onResolveAddDelete with Theirs", async () => {
    const onResolveAddDelete = vi.fn();
    const client = fakeClient({
      getConflictHunks: async () => {
        throw new Error("'binary.dat' is an add/delete conflict, not a text conflict");
      },
    });

    render(
      <ConflictResolutionPane
        repoPath={TEST_REPO_PATH}
        client={client}
        path="binary.dat"
        onResolve={vi.fn()}
        onResolveAddDelete={onResolveAddDelete}
      />,
    );

    await waitFor(() => screen.getByText("Keep their version"));
    fireEvent.click(screen.getByText("Keep their version"));

    expect(onResolveAddDelete).toHaveBeenCalledWith("binary.dat", "Theirs");
  });

  it("still shows a plain error message for an unrelated failure", async () => {
    const client = fakeClient({
      getConflictHunks: async () => {
        throw new Error("network error");
      },
    });

    render(
      <ConflictResolutionPane
        repoPath={TEST_REPO_PATH}
        client={client}
        path="shared.txt"
        onResolve={vi.fn()}
        onResolveAddDelete={vi.fn()}
      />,
    );

    await waitFor(() => screen.getByRole("alert"));
    expect(screen.getByRole("alert")).toHaveTextContent("network error");
    expect(screen.queryByText("Keep our version")).not.toBeInTheDocument();
  });
});

describe("ConflictResolutionPane whole-file header", () => {
  function renderPane(getConflictHunks: RepoClient["getConflictHunks"]) {
    const onResolveWholeFile = vi.fn();
    const onMarkResolved = vi.fn();
    render(
      <ConflictResolutionPane
        repoPath={TEST_REPO_PATH}
        client={fakeClient({ getConflictHunks })}
        path="shared.txt"
        onResolve={vi.fn()}
        onResolveAddDelete={vi.fn()}
        onResolveWholeFile={onResolveWholeFile}
        onMarkResolved={onMarkResolved}
      />,
    );
    return { onResolveWholeFile, onMarkResolved };
  }

  it("offers whole-file ours/theirs and Mark resolved for a text conflict", async () => {
    const { onResolveWholeFile, onMarkResolved } = renderPane(async () => segments);
    const toolbar = await screen.findByRole("group", { name: "Whole file" });
    const ours = within(toolbar).getByRole("button", { name: "Use ours (whole file)" });
    await waitFor(() => expect(ours).toBeEnabled());
    expect(ours).toHaveAttribute("title", "Replace the file with the current branch's version (HEAD).");
    expect(within(toolbar).getByRole("button", { name: "Use theirs (whole file)" })).toHaveAttribute(
      "title",
      "Replace the file with the incoming version.",
    );
    fireEvent.click(ours);
    fireEvent.click(within(toolbar).getByRole("button", { name: "Use theirs (whole file)" }));
    expect(onResolveWholeFile.mock.calls).toEqual([
      ["shared.txt", "Ours"],
      ["shared.txt", "Theirs"],
    ]);
    fireEvent.click(within(toolbar).getByRole("button", { name: "Mark resolved" }));
    expect(onMarkResolved).toHaveBeenCalledTimes(1);
  });

  it("disables the header until the conflict has loaded", () => {
    renderPane(() => new Promise(() => {}));
    expect(screen.getByRole("button", { name: "Use ours (whole file)" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Mark resolved" })).toBeDisabled();
  });

  it("keeps the header for add/delete conflicts next to their own buttons", async () => {
    renderPane(() => Promise.reject("'x' is an add/delete conflict, not a text conflict"));
    expect(await screen.findByRole("button", { name: "Keep our version" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mark resolved" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Use theirs (whole file)" })).toBeEnabled();
  });

  it("keeps the header when the hunks fail for another reason (binary conflict)", async () => {
    const { onResolveWholeFile } = renderPane(() => Promise.reject("stream did not contain valid UTF-8"));
    expect(await screen.findByRole("alert")).toHaveTextContent("stream did not contain valid UTF-8");
    fireEvent.click(screen.getByRole("button", { name: "Use theirs (whole file)" }));
    expect(onResolveWholeFile).toHaveBeenCalledWith("shared.txt", "Theirs");
  });
});
