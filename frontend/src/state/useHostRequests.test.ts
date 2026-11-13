import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { HostRequest } from "../ipc/RepoClient";
import { onWorkspaceRequest, takeCommitRequest, takeHistoryRequest, type WorkspaceRequest } from "../lib/workspaceRequests";
import { fakeRepoClient } from "../test/fakeRepoClient";
import { chordsFor } from "../lib/keybindings";
import { resetAllShortcuts } from "../lib/keymap";
import { useHostRequests } from "./useHostRequests";

describe("useHostRequests (#677, #678, #679)", () => {
  let stopListening: (() => void) | undefined;
  afterEach(() => {
    stopListening?.();
    takeHistoryRequest("/repos/app");
    takeCommitRequest("/repos/app");
  });

  function setup(ready: boolean) {
    let emit: (request: HostRequest) => void = () => {};
    const subscribe = vi.fn((listener: (request: HostRequest) => void) => {
      emit = listener;
      return () => {};
    });
    const client = fakeRepoClient({ subscribeHostRequests: subscribe });
    const opened: string[] = [];
    const openRepoTab = vi.fn(async (path: string) => {
      opened.push(path);
    });
    const hook = renderHook(({ isReady }) => useHostRequests(client, isReady, openRepoTab), { initialProps: { isReady: ready } });
    return { emit: (request: HostRequest) => emit(request), subscribe, opened, hook };
  }

  it("listens only once the restored tabs are open", () => {
    const { subscribe, hook } = setup(false);
    expect(subscribe).not.toHaveBeenCalled();
    hook.rerender({ isReady: true });
    expect(subscribe).toHaveBeenCalledOnce();
  });

  it("opens the repositories so that the first one ends up active", async () => {
    const { emit, opened } = setup(true);
    act(() => emit({ kind: "openRepositories", paths: ["/repos/app", "/repos/lib"] }));
    await waitFor(() => expect(opened).toEqual(["/repos/lib", "/repos/app"]));
  });

  it("opens the repository's tab and leaves the history request for its workspace", async () => {
    const { emit, opened } = setup(true);
    act(() => emit({ kind: "showHistory", repoPath: "/repos/app", path: "src/a.ts", lines: { startLine: 2, endLine: 4 } }));
    await waitFor(() => expect(opened).toEqual(["/repos/app"]));
    expect(takeHistoryRequest("/repos/lib")).toBeNull();
    expect(takeHistoryRequest("/repos/app")).toEqual({
      repoPath: "/repos/app",
      path: "src/a.ts",
      lines: { startLine: 2, endLine: 4 },
    });
    expect(takeHistoryRequest("/repos/app")).toBeNull();
  });

  it("opens the repository's tab and leaves the commit to select for its workspace (#759)", async () => {
    const { emit, opened } = setup(true);
    act(() => emit({ kind: "showCommit", repoPath: "/repos/app", commitId: "abc1234" }));
    await waitFor(() => expect(opened).toEqual(["/repos/app"]));
    expect(takeCommitRequest("/repos/lib")).toBeNull();
    expect(takeCommitRequest("/repos/app")).toEqual({ repoPath: "/repos/app", commitId: "abc1234" });
    expect(takeCommitRequest("/repos/app")).toBeNull();
  });

  it("hands a shortcut to the active workspace", async () => {
    const received: WorkspaceRequest[] = [];
    stopListening = onWorkspaceRequest((request) => received.push(request));
    const { emit } = setup(true);
    act(() => emit({ kind: "shortcut", id: "tab-by-number", index: 2 }));
    await waitFor(() => expect(received).toEqual([{ kind: "shortcut", id: "tab-by-number", index: 2 }]));
  });

  it("takes the keys VSCode runs the shortcuts with (#900)", async () => {
    document.documentElement.dataset.host = "vscode";
    try {
      const { emit } = setup(true);
      act(() => emit({ kind: "keybindings", keys: { refresh: ["f4"] } }));
      await waitFor(() => expect(chordsFor("refresh", false)).toEqual([{ key: "F4" }]));
    } finally {
      delete document.documentElement.dataset.host;
      resetAllShortcuts();
    }
  });
});
