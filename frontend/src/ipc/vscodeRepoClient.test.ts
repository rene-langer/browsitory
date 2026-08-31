import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RepoClient } from "./RepoClient";

describe("vscodeRepoClient", () => {
  let postMessage: ReturnType<typeof vi.fn>;
  let vscodeRepoClient: RepoClient;

  beforeEach(async () => {
    vi.resetModules();
    postMessage = vi.fn();
    (
      globalThis as unknown as { acquireVsCodeApi: () => { postMessage: typeof postMessage } }
    ).acquireVsCodeApi = () => ({ postMessage });
    ({ vscodeRepoClient } = await import("./vscodeRepoClient"));
  });

  function respond(id: number, result: unknown) {
    window.dispatchEvent(new MessageEvent("message", { data: { jsonrpc: "2.0", id, result } }));
  }

  function respondWithError(id: number, message: string) {
    window.dispatchEvent(
      new MessageEvent("message", {
        data: { jsonrpc: "2.0", id, error: { code: -32000, message } },
      }),
    );
  }

  it("posts a JSON-RPC request and resolves on a matching reply", async () => {
    const promise = vscodeRepoClient.getStatus("/repo");

    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_status",
      params: { repoPath: "/repo" },
    });
    respond(1, [{ path: "a.txt", staged: false, kind: "New" }]);

    await expect(promise).resolves.toEqual([{ path: "a.txt", staged: false, kind: "New" }]);
  });

  it("rejects when the sidecar returns a JSON-RPC error", async () => {
    const promise = vscodeRepoClient.getStatus("/missing");

    respondWithError(1, "repo not open: /missing");

    await expect(promise).rejects.toThrow("repo not open: /missing");
  });

  it("correlates concurrent requests by id", async () => {
    const openPromise = vscodeRepoClient.openRepo("/repo");
    const statusPromise = vscodeRepoClient.getStatus("/repo");

    respond(2, [{ path: "b.txt", staged: true, kind: "Modified" }]);
    respond(1, null);

    // A void-returning JSON-RPC method's wire response carries `result: null` (JSON has no
    // `undefined`); the client resolves with that `null` as-is rather than coercing it.
    await expect(openPromise).resolves.toBeNull();
    await expect(statusPromise).resolves.toEqual([
      { path: "b.txt", staged: true, kind: "Modified" },
    ]);
  });

  it("rejects unwired methods without touching postMessage", async () => {
    await expect(vscodeRepoClient.listSubmodules("/repo")).rejects.toThrow(
      "listSubmodules is not implemented yet",
    );
    expect(postMessage).not.toHaveBeenCalled();
  });

  it("wires listRecentRepos", async () => {
    const promise = vscodeRepoClient.listRecentRepos();
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "list_recent_repos",
      params: {},
    });
    respond(1, ["/repos/a"]);
    await expect(promise).resolves.toEqual(["/repos/a"]);
  });

  it("wires listOpenRepos", async () => {
    const promise = vscodeRepoClient.listOpenRepos();
    respond(1, { entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" });
    await expect(promise).resolves.toEqual({
      entries: [{ path: "/repos/a", workspaceId: null }],
      activePath: "/repos/a",
    });
  });

  it("wires persistOpenRepos", async () => {
    const promise = vscodeRepoClient.persistOpenRepos(
      [{ path: "/repos/a", workspaceId: null }],
      "/repos/a",
    );
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "persist_open_repos",
      params: { entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();
  });

  it("wires scanReposInRoot", async () => {
    const promise = vscodeRepoClient.scanReposInRoot("/repos");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "scan_repos_in_root",
      params: { root: "/repos" },
    });
    respond(1, ["/repos/a"]);
    await expect(promise).resolves.toEqual(["/repos/a"]);
  });

  it("wires listWorkspaces", async () => {
    const promise = vscodeRepoClient.listWorkspaces();
    respond(1, [{ id: "w1", name: "Suite", rootPath: "/repos/suite", memberPaths: [] }]);
    await expect(promise).resolves.toEqual([
      { id: "w1", name: "Suite", rootPath: "/repos/suite", memberPaths: [] },
    ]);
  });

  it("wires saveWorkspace", async () => {
    const promise = vscodeRepoClient.saveWorkspace("Suite", "/repos/suite", ["/repos/suite/api"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "save_workspace",
      params: { name: "Suite", root: "/repos/suite", members: ["/repos/suite/api"] },
    });
    respond(1, "w1");
    await expect(promise).resolves.toBe("w1");
  });

  it("wires updateWorkspace", async () => {
    const promise = vscodeRepoClient.updateWorkspace("w1", "Renamed", ["/repos/suite/web"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "update_workspace",
      params: { id: "w1", name: "Renamed", members: ["/repos/suite/web"] },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();
  });

  it("wires deleteWorkspace", async () => {
    const promise = vscodeRepoClient.deleteWorkspace("w1");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "delete_workspace",
      params: { id: "w1" },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();
  });

  it("wires getGraphBranchSelection and setGraphBranchSelection", async () => {
    const getPromise = vscodeRepoClient.getGraphBranchSelection("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_graph_branch_selection",
      params: { repoPath: "/repo" },
    });
    respond(1, ["main"]);
    await expect(getPromise).resolves.toEqual(["main"]);

    const setPromise = vscodeRepoClient.setGraphBranchSelection("/repo", ["main", "feature"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "set_graph_branch_selection",
      params: { repoPath: "/repo", selectedBranches: ["main", "feature"] },
    });
    respond(2, null);
    await expect(setPromise).resolves.toBeNull();
  });

  it("wires getCommitFiles", async () => {
    const promise = vscodeRepoClient.getCommitFiles("/repo", "abc123");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_commit_files",
      params: { repoPath: "/repo", commitId: "abc123" },
    });
    respond(1, ["a.txt"]);
    await expect(promise).resolves.toEqual(["a.txt"]);
  });

  it("wires stageFile and unstageFile", async () => {
    const stagePromise = vscodeRepoClient.stageFile("/repo", "a.txt");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "stage_file",
      params: { repoPath: "/repo", path: "a.txt" },
    });
    respond(1, null);
    await expect(stagePromise).resolves.toBeNull();

    const unstagePromise = vscodeRepoClient.unstageFile("/repo", "a.txt");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "unstage_file",
      params: { repoPath: "/repo", path: "a.txt" },
    });
    respond(2, null);
    await expect(unstagePromise).resolves.toBeNull();
  });

  it("wires stageHunk, unstageHunk, and discardHunk", async () => {
    const stagePromise = vscodeRepoClient.stageHunk("/repo", "a.txt", 1, 1);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "stage_hunk",
      params: { repoPath: "/repo", path: "a.txt", oldStart: 1, newStart: 1 },
    });
    respond(1, null);
    await expect(stagePromise).resolves.toBeNull();

    const unstagePromise = vscodeRepoClient.unstageHunk("/repo", "a.txt", 1, 1);
    respond(2, null);
    await expect(unstagePromise).resolves.toBeNull();

    const discardPromise = vscodeRepoClient.discardHunk("/repo", "a.txt", 1, 1);
    respond(3, null);
    await expect(discardPromise).resolves.toBeNull();
  });

  it("wires commit", async () => {
    const promise = vscodeRepoClient.commit("/repo", "message");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "commit",
      params: { repoPath: "/repo", message: "message" },
    });
    respond(1, "abc123");
    await expect(promise).resolves.toBe("abc123");
  });

  it("wires listBranches", async () => {
    const promise = vscodeRepoClient.listBranches("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "list_branches",
      params: { repoPath: "/repo" },
    });
    respond(1, [{ name: "main", isCurrent: true }]);
    await expect(promise).resolves.toEqual([{ name: "main", isCurrent: true }]);
  });

  it("wires createBranch, switchBranch, renameBranch, and deleteBranch", async () => {
    const createPromise = vscodeRepoClient.createBranch("/repo", "feature", "HEAD");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "create_branch",
      params: { repoPath: "/repo", name: "feature", startPoint: "HEAD" },
    });
    respond(1, null);
    await expect(createPromise).resolves.toBeNull();

    const switchPromise = vscodeRepoClient.switchBranch("/repo", "feature");
    respond(2, null);
    await expect(switchPromise).resolves.toBeNull();

    const renamePromise = vscodeRepoClient.renameBranch("/repo", "feature", "renamed");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "rename_branch",
      params: { repoPath: "/repo", oldName: "feature", newName: "renamed" },
    });
    respond(3, null);
    await expect(renamePromise).resolves.toBeNull();

    const deletePromise = vscodeRepoClient.deleteBranch("/repo", "renamed", true);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 4,
      method: "delete_branch",
      params: { repoPath: "/repo", name: "renamed", force: true },
    });
    respond(4, null);
    await expect(deletePromise).resolves.toBeNull();
  });

  it("wires listWorktrees", async () => {
    const promise = vscodeRepoClient.listWorktrees("/repo");
    respond(1, [
      { name: "main", path: "/repo", head: "refs/heads/main", isMain: true, isLocked: false, isPrunable: false },
    ]);
    await expect(promise).resolves.toEqual([
      { name: "main", path: "/repo", head: "refs/heads/main", isMain: true, isLocked: false, isPrunable: false },
    ]);
  });

  it("wires createWorktree, removeWorktree, and pruneWorktrees", async () => {
    const createPromise = vscodeRepoClient.createWorktree("/repo", "feature-tree", "/repo-feature", "feature", "HEAD");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "create_worktree",
      params: { repoPath: "/repo", name: "feature-tree", path: "/repo-feature", branch: "feature", startPoint: "HEAD" },
    });
    respond(1, null);
    await expect(createPromise).resolves.toBeNull();

    const removePromise = vscodeRepoClient.removeWorktree("/repo", "feature-tree");
    respond(2, null);
    await expect(removePromise).resolves.toBeNull();

    const prunePromise = vscodeRepoClient.pruneWorktrees("/repo");
    respond(3, null);
    await expect(prunePromise).resolves.toBeNull();
  });
});
