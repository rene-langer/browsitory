import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { tauriRepoClient } from "./tauriRepoClient";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
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
    const fetch = tauriRepoClient.fetchRemote(TEST_REPO_PATH, "origin");
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
});
