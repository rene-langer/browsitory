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
    await expect(vscodeRepoClient.getBlame("/repo", "abc123", "file.txt")).rejects.toThrow(
      "getBlame is not implemented yet",
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

  it("wires listSubmodules, initSubmodule, and updateSubmodule", async () => {
    const listPromise = vscodeRepoClient.listSubmodules("/repo");
    respond(1, [{ path: "deps/child", url: null, gitlinkId: null, initialized: false, headId: null }]);
    await expect(listPromise).resolves.toEqual([
      { path: "deps/child", url: null, gitlinkId: null, initialized: false, headId: null },
    ]);

    const initPromise = vscodeRepoClient.initSubmodule("/repo", "deps/child");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "init_submodule",
      params: { repoPath: "/repo", path: "deps/child" },
    });
    respond(2, null);
    await expect(initPromise).resolves.toBeNull();

    const updatePromise = vscodeRepoClient.updateSubmodule("/repo", "deps/child", true);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "update_submodule",
      params: { repoPath: "/repo", path: "deps/child", recursive: true },
    });
    respond(3, null);
    await expect(updatePromise).resolves.toBeNull();
  });

  it("wires listReflogRefs, getReflog, and restoreReflogEntry", async () => {
    const refsPromise = vscodeRepoClient.listReflogRefs("/repo");
    respond(1, ["HEAD"]);
    await expect(refsPromise).resolves.toEqual(["HEAD"]);

    const reflogPromise = vscodeRepoClient.getReflog("/repo", "HEAD");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "get_reflog",
      params: { repoPath: "/repo", reference: "HEAD" },
    });
    respond(2, [
      {
        reference: "HEAD",
        oldId: "1111111",
        newId: "2222222",
        committerName: "Test User",
        committerEmail: "test@example.com",
        timestamp: 1_725_000_000,
        message: "commit: second commit",
        summary: "second commit",
      },
    ]);
    await expect(reflogPromise).resolves.toHaveLength(1);

    const restorePromise = vscodeRepoClient.restoreReflogEntry("/repo", "HEAD", "1111111");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "restore_reflog_entry",
      params: { repoPath: "/repo", reference: "HEAD", newId: "1111111" },
    });
    respond(3, null);
    await expect(restorePromise).resolves.toBeNull();
  });

  it("wires listRemotes, listRemoteBranches, and upstream methods", async () => {
    const remotesPromise = vscodeRepoClient.listRemotes("/repo");
    respond(1, [{ name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null, authMode: null, authUsername: null }]);
    await expect(remotesPromise).resolves.toHaveLength(1);

    const branchesPromise = vscodeRepoClient.listRemoteBranches("/repo", "origin");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "list_remote_branches",
      params: { repoPath: "/repo", remoteName: "origin" },
    });
    respond(2, ["main"]);
    await expect(branchesPromise).resolves.toEqual(["main"]);

    const currentPromise = vscodeRepoClient.getCurrentUpstream("/repo");
    respond(3, null);
    await expect(currentPromise).resolves.toBeNull();

    const upstreamsPromise = vscodeRepoClient.getRemoteUpstreams("/repo", "origin");
    respond(4, []);
    await expect(upstreamsPromise).resolves.toEqual([]);
  });

  it("wires addRemote and rejects embedded credentials before posting", async () => {
    const promise = vscodeRepoClient.addRemote("/repo", "origin", "https://example.com/r.git", null);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "add_remote",
      params: { repoPath: "/repo", name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();

    expect(() =>
      vscodeRepoClient.addRemote("/repo", "origin", "https://alice:secret@example.com/r.git", null),
    ).toThrow("Remote URLs must not contain embedded credentials");
  });

  it("wires renameRemote, updateRemoteUrls, and removeRemote", async () => {
    const renamePromise = vscodeRepoClient.renameRemote("/repo", "origin", "upstream");
    respond(1, null);
    await expect(renamePromise).resolves.toBeNull();

    const updatePromise = vscodeRepoClient.updateRemoteUrls("/repo", "upstream", "https://example.com/r2.git", null);
    respond(2, null);
    await expect(updatePromise).resolves.toBeNull();

    const removePromise = vscodeRepoClient.removeRemote("/repo", "upstream", true);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "remove_remote",
      params: { repoPath: "/repo", name: "upstream", clearUpstreams: true },
    });
    respond(3, null);
    await expect(removePromise).resolves.toBeNull();
  });

  it("wires https credential and auth mode methods", async () => {
    const savePromise = vscodeRepoClient.saveHttpsCredential("/repo", "origin", "alice", "secret");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "save_https_credential",
      params: { repoPath: "/repo", remoteName: "origin", username: "alice", token: "secret" },
    });
    respond(1, null);
    await expect(savePromise).resolves.toBeNull();

    const forgetPromise = vscodeRepoClient.forgetHttpsCredential("/repo", "origin");
    respond(2, null);
    await expect(forgetPromise).resolves.toBeNull();

    const authPromise = vscodeRepoClient.setRemoteAuthMode("/repo", "origin", "HttpsToken", "alice");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "set_remote_auth_mode",
      params: { repoPath: "/repo", remoteName: "origin", mode: "HttpsToken", username: "alice" },
    });
    respond(3, null);
    await expect(authPromise).resolves.toBeNull();
  });

  it("wires setCurrentUpstream and clearCurrentUpstream", async () => {
    const setPromise = vscodeRepoClient.setCurrentUpstream("/repo", "origin", "main");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "set_current_upstream",
      params: { repoPath: "/repo", remoteName: "origin", remoteBranch: "main" },
    });
    respond(1, null);
    await expect(setPromise).resolves.toBeNull();

    const clearPromise = vscodeRepoClient.clearCurrentUpstream("/repo");
    respond(2, null);
    await expect(clearPromise).resolves.toBeNull();
  });

  it("wires listTags, createTag, and deleteTag", async () => {
    const createPromise = vscodeRepoClient.createTag("/repo", "v1.0.0", "first release");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "create_tag",
      params: { repoPath: "/repo", name: "v1.0.0", message: "first release" },
    });
    respond(1, null);
    await expect(createPromise).resolves.toBeNull();

    const listPromise = vscodeRepoClient.listTags("/repo");
    respond(2, [
      { name: "v1.0.0", targetId: "abc", annotated: true, message: "first release", taggerName: "Test User", timestamp: 1_725_000_000 },
    ]);
    await expect(listPromise).resolves.toHaveLength(1);

    const deletePromise = vscodeRepoClient.deleteTag("/repo", "v1.0.0");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "delete_tag",
      params: { repoPath: "/repo", name: "v1.0.0" },
    });
    respond(3, null);
    await expect(deletePromise).resolves.toBeNull();
  });

  function notify(method: string, params: unknown) {
    window.dispatchEvent(new MessageEvent("message", { data: { jsonrpc: "2.0", method, params } }));
  }

  const progress = (phase: string) => ({
    operationId: "fetch-1",
    operation: "Fetch",
    phase,
    errorKind: null,
    current: 0,
    total: 0,
    receivedBytes: 0,
    message: null,
  });

  it("wires fetchRemote, pushCurrentBranch, and pushTags", async () => {
    const fetchPromise = vscodeRepoClient.fetchRemote("/repo", "origin");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "fetch_remote",
      params: { repoPath: "/repo", remoteName: "origin" },
    });
    respond(1, "fetch-1");
    await expect(fetchPromise).resolves.toBe("fetch-1");

    const pushPromise = vscodeRepoClient.pushCurrentBranch("/repo", "origin");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "push_current_branch",
      params: { repoPath: "/repo", remoteName: "origin" },
    });
    respond(2, "push-1");
    await expect(pushPromise).resolves.toBe("push-1");

    const tagsPromise = vscodeRepoClient.pushTags("/repo", "origin", ["v1.0.0"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "push_tags",
      params: { repoPath: "/repo", remoteName: "origin", names: ["v1.0.0"] },
    });
    respond(3, "push-2");
    await expect(tagsPromise).resolves.toBe("push-2");
  });

  it("wires pullCurrentUpstream", async () => {
    const promise = vscodeRepoClient.pullCurrentUpstream("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "pull_current_upstream",
      params: { repoPath: "/repo" },
    });
    respond(1, { kind: "FastForwarded", upstreamRef: "refs/remotes/origin/main" });
    await expect(promise).resolves.toEqual({
      kind: "FastForwarded",
      upstreamRef: "refs/remotes/origin/main",
    });
  });

  it("delivers transferProgress notifications to subscribed listeners and stops after unsubscribe", () => {
    const received: unknown[] = [];
    const unsubscribe = vscodeRepoClient.subscribeTransferProgress((p) => received.push(p));

    notify("transferProgress", progress("Starting"));
    expect(received).toEqual([progress("Starting")]);

    unsubscribe();
    notify("transferProgress", progress("Completed"));
    expect(received).toHaveLength(1);
  });

  it("fans a transferProgress notification out to every subscribed listener", () => {
    const first: unknown[] = [];
    const second: unknown[] = [];
    const unsubscribeFirst = vscodeRepoClient.subscribeTransferProgress((p) => first.push(p));
    vscodeRepoClient.subscribeTransferProgress((p) => second.push(p));

    notify("transferProgress", progress("Receiving"));

    expect(first).toHaveLength(1);
    expect(second).toHaveLength(1);

    // Unsubscribing one listener leaves the other subscribed.
    unsubscribeFirst();
    notify("transferProgress", progress("Completed"));
    expect(first).toHaveLength(1);
    expect(second).toHaveLength(2);
  });

  it("ignores notifications for methods it doesn't recognize", () => {
    const received: unknown[] = [];
    vscodeRepoClient.subscribeTransferProgress((p) => received.push(p));

    notify("somethingElse", { anything: true });

    expect(received).toHaveLength(0);
  });

  it("never settles a pending request from a notification, and vice versa", async () => {
    const received: unknown[] = [];
    vscodeRepoClient.subscribeTransferProgress((p) => received.push(p));
    const promise = vscodeRepoClient.fetchRemote("/repo", "origin");
    let settled = false;
    void promise.then(() => {
      settled = true;
    });

    // A notification has a `method` and no `id`, so it can never be matched to request 1.
    notify("transferProgress", progress("Starting"));
    await Promise.resolve();
    expect(settled).toBe(false);
    expect(received).toHaveLength(1);

    // ...and the response, which has an `id`, is never mistaken for a notification.
    respond(1, "fetch-1");
    await expect(promise).resolves.toBe("fetch-1");
    expect(received).toHaveLength(1);
  });

  it("registers the message listener even when subscribeTransferProgress runs before any call", () => {
    const received: unknown[] = [];
    vscodeRepoClient.subscribeTransferProgress((p) => received.push(p));

    notify("transferProgress", progress("Starting"));

    expect(received).toHaveLength(1);
    expect(postMessage).not.toHaveBeenCalled();
  });

  it("wires listStashes, saveStash, applyStash, and dropStash", async () => {
    const savePromise = vscodeRepoClient.saveStash("/repo");
    respond(1, null);
    await expect(savePromise).resolves.toBeNull();

    const listPromise = vscodeRepoClient.listStashes("/repo");
    respond(2, [{ index: 0, message: "WIP", commitId: "abc123" }]);
    await expect(listPromise).resolves.toHaveLength(1);

    const applyPromise = vscodeRepoClient.applyStash("/repo", 0);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "apply_stash",
      params: { repoPath: "/repo", index: 0 },
    });
    respond(3, null);
    await expect(applyPromise).resolves.toBeNull();

    const dropPromise = vscodeRepoClient.dropStash("/repo", 0);
    respond(4, null);
    await expect(dropPromise).resolves.toBeNull();
  });
});
