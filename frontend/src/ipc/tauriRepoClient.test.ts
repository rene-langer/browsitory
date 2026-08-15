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

describe("tauriRepoClient remote URL validation", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it.each([
    [
      "fetch URL passed to addRemote",
      () => tauriRepoClient.addRemote("origin", "https://user:secret@example.com/repo.git", null),
    ],
    [
      "push URL passed to updateRemoteUrls",
      () =>
        tauriRepoClient.updateRemoteUrls(
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

    await tauriRepoClient.saveHttpsCredential("origin", "rene", "token-123");

    expect(invoke).toHaveBeenCalledWith("save_https_credential", {
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

    await tauriRepoClient.listWorktrees();
    await tauriRepoClient.createWorktree(
      "feature-tree",
      "/repos/project-feature",
      "feature",
      null,
    );
    await tauriRepoClient.removeWorktree("feature-tree");
    await tauriRepoClient.pruneWorktrees();

    expect(invoke).toHaveBeenNthCalledWith(1, "list_worktrees");
    expect(invoke).toHaveBeenNthCalledWith(2, "create_worktree", {
      name: "feature-tree",
      path: "/repos/project-feature",
      branch: "feature",
      startPoint: null,
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "remove_worktree", {
      name: "feature-tree",
    });
    expect(invoke).toHaveBeenNthCalledWith(4, "prune_worktrees");
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
    const fetch = tauriRepoClient.fetchRemote("origin");
    await Promise.resolve();
    expect(invoke).not.toHaveBeenCalled();

    resolveProgress(() => {});
    await Promise.resolve();
    expect(invoke).not.toHaveBeenCalled();

    resolveCompleted(() => {});
    await expect(fetch).resolves.toBe("fetch-42");
    expect(invoke).toHaveBeenCalledWith("fetch_remote", { remoteName: "origin" });
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
