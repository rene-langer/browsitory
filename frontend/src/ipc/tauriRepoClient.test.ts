import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { error as logError } from "@tauri-apps/plugin-log";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { tauriRepoClient } from "./tauriRepoClient";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-log", () => ({
  error: vi.fn(async () => {}),
}));

const TEST_REPO_PATH = "/repo";

describe("tauriRepoClient remote URL validation", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it.each([
    [
      "fetch URL passed to addRemote",
      () =>
        tauriRepoClient.addRemote(
          TEST_REPO_PATH,
          "origin",
          "https://user:secret@example.com/repo.git",
          null,
        ),
    ],
    [
      "push URL passed to updateRemoteUrls",
      () =>
        tauriRepoClient.updateRemoteUrls(
          TEST_REPO_PATH,
          "origin",
          "https://example.com/repo.git",
          "HTTPS://user@example.com/repo.git",
        ),
    ],
  ])("rejects HTTP(S) userinfo in the %s before invoking Tauri", async (_description, operation) => {
    await expect(async () => operation()).rejects.toThrow(
      "Remote URLs must not contain embedded credentials",
    );
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("tauriRepoClient failure logging", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(logError).mockClear();
  });

  it("writes logFrontendError calls into the Tauri plugin-log sink", async () => {
    await tauriRepoClient.logFrontendError("Uncaught error", new Error("boom"));

    expect(logError).toHaveBeenCalledWith("Uncaught error: Error: boom");
  });

  it("logs a failed invoke through the same sink and still rejects", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("backend exploded"));

    await expect(tauriRepoClient.getStatus(TEST_REPO_PATH)).rejects.toThrow("backend exploded");

    expect(logError).toHaveBeenCalledWith("IPC get_status failed: Error: backend exploded");
  });
});

describe("tauriRepoClient credentials", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("sends a token only as the direct save command argument", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await tauriRepoClient.saveHttpsCredential(TEST_REPO_PATH, "origin", "rene", "token-123");

    expect(invoke).toHaveBeenCalledWith("save_https_credential", {
      repoPath: TEST_REPO_PATH,
      remoteName: "origin",
      username: "rene",
      token: "token-123",
    });
  });
});

describe("tauriRepoClient worktrees", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("maps worktree operations to their Tauri commands", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await tauriRepoClient.listWorktrees(TEST_REPO_PATH);
    await tauriRepoClient.createWorktree(
      TEST_REPO_PATH,
      "feature-tree",
      "/repos/project-feature",
      "feature",
      null,
    );
    await tauriRepoClient.removeWorktree(TEST_REPO_PATH, "feature-tree");
    await tauriRepoClient.pruneWorktrees(TEST_REPO_PATH);

    expect(invoke).toHaveBeenNthCalledWith(1, "list_worktrees", { repoPath: TEST_REPO_PATH });
    expect(invoke).toHaveBeenNthCalledWith(2, "create_worktree", {
      repoPath: TEST_REPO_PATH,
      name: "feature-tree",
      path: "/repos/project-feature",
      branch: "feature",
      startPoint: null,
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "remove_worktree", {
      repoPath: TEST_REPO_PATH,
      name: "feature-tree",
    });
    expect(invoke).toHaveBeenNthCalledWith(4, "prune_worktrees", { repoPath: TEST_REPO_PATH });
  });
});

describe("tauriRepoClient submodules", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("maps submodule operations to their Tauri commands", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await tauriRepoClient.listSubmodules(TEST_REPO_PATH);
    await tauriRepoClient.initSubmodule(TEST_REPO_PATH, "deps/child");
    await tauriRepoClient.updateSubmodule(TEST_REPO_PATH, "deps/child", true);

    expect(invoke).toHaveBeenNthCalledWith(1, "list_submodules", { repoPath: TEST_REPO_PATH });
    expect(invoke).toHaveBeenNthCalledWith(2, "init_submodule", {
      repoPath: TEST_REPO_PATH,
      path: "deps/child",
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "update_submodule", {
      repoPath: TEST_REPO_PATH,
      path: "deps/child",
      recursive: true,
    });
  });
});

describe("tauriRepoClient reflog", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("maps reflog operations to their Tauri commands", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await tauriRepoClient.listReflogRefs(TEST_REPO_PATH);
    await tauriRepoClient.getReflog(TEST_REPO_PATH, "HEAD");
    await tauriRepoClient.restoreReflogEntry(TEST_REPO_PATH, "HEAD", "0123456789abcdef");

    expect(invoke).toHaveBeenNthCalledWith(1, "list_reflog_refs", { repoPath: TEST_REPO_PATH });
    expect(invoke).toHaveBeenNthCalledWith(2, "get_reflog", {
      repoPath: TEST_REPO_PATH,
      reference: "HEAD",
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "restore_reflog_entry", {
      repoPath: TEST_REPO_PATH,
      reference: "HEAD",
      newId: "0123456789abcdef",
    });
  });
});

describe("tauriRepoClient transfer progress subscription", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it("waits for both transfer listeners before invoking a fetch", async () => {
    let resolveProgress!: (unlisten: () => void) => void;
    let resolveCompleted!: (unlisten: () => void) => void;
    vi.mocked(listen).mockImplementation((event) =>
      new Promise((resolve) => {
        if (event === "transfer-progress") resolveProgress = resolve;
        if (event === "transfer-complete") resolveCompleted = resolve;
      }),
    );
    vi.mocked(invoke).mockResolvedValue("fetch-42");

    tauriRepoClient.subscribeTransferProgress(() => {});
    const fetch = tauriRepoClient.fetchRemote(TEST_REPO_PATH, "origin", null);
    await Promise.resolve();
    expect(invoke).not.toHaveBeenCalled();

    resolveProgress(() => {});
    await Promise.resolve();
    expect(invoke).not.toHaveBeenCalled();

    resolveCompleted(() => {});
    await expect(fetch).resolves.toBe("fetch-42");
    expect(invoke).toHaveBeenCalledWith("fetch_remote", {
      repoPath: TEST_REPO_PATH,
      remoteName: "origin",
      prune: null,
    });
  });

  it("invokes the new transfer commands with camelCase arguments once the listeners are ready", async () => {
    vi.mocked(listen).mockResolvedValue(() => {});
    vi.mocked(invoke).mockResolvedValue("op-1");
    tauriRepoClient.subscribeTransferProgress(() => {});

    await tauriRepoClient.fetchRemote(TEST_REPO_PATH, "origin", false);
    expect(invoke).toHaveBeenLastCalledWith("fetch_remote", { repoPath: TEST_REPO_PATH, remoteName: "origin", prune: false });
    await tauriRepoClient.fetchAllRemotes(TEST_REPO_PATH, true);
    expect(invoke).toHaveBeenLastCalledWith("fetch_all_remotes", { repoPath: TEST_REPO_PATH, prune: true });
    await tauriRepoClient.pushBranch(TEST_REPO_PATH, "origin", "topic", "published", true);
    expect(invoke).toHaveBeenLastCalledWith("push_branch", {
      repoPath: TEST_REPO_PATH,
      remoteName: "origin",
      localBranch: "topic",
      remoteBranch: "published",
      setUpstream: true,
    });
    await tauriRepoClient.deleteRemoteBranch(TEST_REPO_PATH, "origin", "topic");
    expect(invoke).toHaveBeenLastCalledWith("delete_remote_branch", {
      repoPath: TEST_REPO_PATH,
      remoteName: "origin",
      branchName: "topic",
    });
  });

  it("invokes cancel_transfer without waiting for the transfer listeners", async () => {
    // Unlike fetch/push/pull this starts no transfer, so it must not be gated on the listener
    // handshake — a cancel that waited for listeners that never resolve could never be sent.
    vi.mocked(listen).mockImplementation(() => new Promise(() => {}));
    vi.mocked(invoke).mockResolvedValue(null);

    tauriRepoClient.subscribeTransferProgress(() => {});
    await expect(
      tauriRepoClient.cancelTransfer(TEST_REPO_PATH, "fetch-42"),
    ).resolves.toBeNull();

    expect(invoke).toHaveBeenCalledWith("cancel_transfer", {
      repoPath: TEST_REPO_PATH,
      operationId: "fetch-42",
    });
  });

  it("normalizes progress events and unregisters its listeners", async () => {
    const unlisten = vi.fn();
    let progressListener: ((event: { payload: unknown }) => void) | undefined;
    let completedListener: ((event: { payload: unknown }) => void) | undefined;
    vi.mocked(listen).mockImplementation(async (event, listener) => {
      if (event === "transfer-progress") progressListener = listener as typeof progressListener;
      if (event === "transfer-complete") completedListener = listener as typeof completedListener;
      return unlisten;
    });
    const received = vi.fn();

    const unsubscribe = tauriRepoClient.subscribeTransferProgress(received);
    await vi.waitFor(() => expect(progressListener).toBeDefined());
    await vi.waitFor(() => expect(completedListener).toBeDefined());
    progressListener?.({
      payload: {
        operationId: "fetch-42",
        operation: "Fetch",
        phase: "Receiving",
        errorKind: null,
        current: 2,
        total: 4,
        receivedBytes: 1024,
        message: null,
      },
    });
    completedListener?.({
      payload: {
        operationId: "fetch-42",
        operation: "Fetch",
        phase: "Completed",
        errorKind: null,
        current: 4,
        total: 4,
        receivedBytes: 1024,
        message: null,
      },
    });

    expect(received).toHaveBeenNthCalledWith(1, {
      operationId: "fetch-42",
      operation: "Fetch",
      phase: "Receiving",
      errorKind: null,
      current: 2,
      total: 4,
      receivedBytes: 1024,
      message: null,
    });
    expect(received).toHaveBeenNthCalledWith(2, {
      operationId: "fetch-42",
      operation: "Fetch",
      phase: "Completed",
      errorKind: null,
      current: 4,
      total: 4,
      receivedBytes: 1024,
      message: null,
    });

    unsubscribe();
    expect(unlisten).toHaveBeenCalledTimes(2);
  });

  it("invokes the history-rewrite commands with camelCase args", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await tauriRepoClient.cherryPick("/repo", "abc");
    expect(invoke).toHaveBeenLastCalledWith("cherry_pick", { repoPath: "/repo", commitId: "abc" });
    await tauriRepoClient.revertCommit("/repo", "abc");
    expect(invoke).toHaveBeenLastCalledWith("revert_commit", { repoPath: "/repo", commitId: "abc" });
    await tauriRepoClient.resetToCommit("/repo", "abc", "hard");
    expect(invoke).toHaveBeenLastCalledWith("reset_to_commit", { repoPath: "/repo", commitId: "abc", mode: "hard" });
    await tauriRepoClient.checkoutCommit("/repo", "abc");
    expect(invoke).toHaveBeenLastCalledWith("checkout_commit", { repoPath: "/repo", commitId: "abc" });
    await tauriRepoClient.amendCommit("/repo", "msg");
    expect(invoke).toHaveBeenLastCalledWith("amend_commit", { repoPath: "/repo", message: "msg" });
    await tauriRepoClient.getCommitSignature("/repo", "abc");
    expect(invoke).toHaveBeenLastCalledWith("get_commit_signature", { repoPath: "/repo", commitId: "abc" });
    await tauriRepoClient.getUndoState("/repo");
    expect(invoke).toHaveBeenLastCalledWith("get_undo_state", { repoPath: "/repo" });
    await tauriRepoClient.undo("/repo");
    expect(invoke).toHaveBeenLastCalledWith("undo_last", { repoPath: "/repo" });
    await tauriRepoClient.redo("/repo");
    expect(invoke).toHaveBeenLastCalledWith("redo_last", { repoPath: "/repo" });
    await tauriRepoClient.getHeadInfo("/repo");
    expect(invoke).toHaveBeenLastCalledWith("get_head_info", { repoPath: "/repo" });
    await tauriRepoClient.forcePushCurrentBranch("/repo", "origin");
    expect(invoke).toHaveBeenLastCalledWith("force_push_current_branch", { repoPath: "/repo", remoteName: "origin" });
  });
});

describe("tauriRepoClient diff pane commands", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
  });

  it("invokes the diff-pane commands with camelCase args", async () => {
    const opts = { ignoreWhitespace: true, contextLines: 5 };
    await tauriRepoClient.getWorkingDiff("/repo", "f.txt", false, opts);
    expect(invoke).toHaveBeenLastCalledWith("get_working_diff", { repoPath: "/repo", path: "f.txt", staged: false, opts });
    await tauriRepoClient.getCommitDiff("/repo", "abc", "f.txt", opts);
    expect(invoke).toHaveBeenLastCalledWith("get_commit_diff", { repoPath: "/repo", commitId: "abc", path: "f.txt", opts });
    await tauriRepoClient.stageHunk("/repo", "f.txt", 1, 2, 5);
    expect(invoke).toHaveBeenLastCalledWith("stage_hunk", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5 });
    await tauriRepoClient.unstageHunk("/repo", "f.txt", 1, 2, 5);
    expect(invoke).toHaveBeenLastCalledWith("unstage_hunk", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5 });
    await tauriRepoClient.discardHunk("/repo", "f.txt", 1, 2, 5);
    expect(invoke).toHaveBeenLastCalledWith("discard_hunk", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5 });
    await tauriRepoClient.stageLines("/repo", "f.txt", 1, 2, 5, [3, 4], ["a\n", "+b\n"]);
    expect(invoke).toHaveBeenLastCalledWith("stage_lines", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5, lineIndices: [3, 4], expectedLines: ["a\n", "+b\n"] });
    await tauriRepoClient.unstageLines("/repo", "f.txt", 1, 2, 5, [3], ["x\n"]);
    expect(invoke).toHaveBeenLastCalledWith("unstage_lines", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5, lineIndices: [3], expectedLines: ["x\n"] });
    await tauriRepoClient.discardLines("/repo", "f.txt", 1, 2, 5, [3], ["x\n"]);
    expect(invoke).toHaveBeenLastCalledWith("discard_lines", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5, lineIndices: [3], expectedLines: ["x\n"] });
    await tauriRepoClient.discardFile("/repo", "f.txt");
    expect(invoke).toHaveBeenLastCalledWith("discard_file", { repoPath: "/repo", path: "f.txt" });
    await tauriRepoClient.discardAllUnstaged("/repo", ["a.txt", "nested/"]);
    expect(invoke).toHaveBeenLastCalledWith("discard_all_unstaged", { repoPath: "/repo", paths: ["a.txt", "nested/"] });
    await tauriRepoClient.ignorePath("/repo", "*.log");
    expect(invoke).toHaveBeenLastCalledWith("ignore_path", { repoPath: "/repo", pattern: "*.log" });
    await tauriRepoClient.getDiffSettings();
    expect(invoke).toHaveBeenLastCalledWith("get_diff_settings", undefined);
    const settings = { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true };
    await tauriRepoClient.setDiffSettings(settings);
    expect(invoke).toHaveBeenLastCalledWith("set_diff_settings", { settings });
  });
});

describe("tauriRepoClient commit search", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(logError).mockClear();
  });

  it("invokes search_commits without logging its errors, and cancel_search", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("Search cancelled.");
    await expect(tauriRepoClient.searchCommits("/repo", "author:ada", null, "100:abc")).rejects.toBe(
      "Search cancelled.",
    );
    expect(invoke).toHaveBeenLastCalledWith("search_commits", {
      repoPath: "/repo",
      query: "author:ada",
      refs: null,
      cursor: "100:abc",
    });
    expect(logError).not.toHaveBeenCalled();

    vi.mocked(invoke).mockResolvedValueOnce(null);
    await tauriRepoClient.cancelSearch("/repo");
    expect(invoke).toHaveBeenLastCalledWith("cancel_search", { repoPath: "/repo" });
  });
});
