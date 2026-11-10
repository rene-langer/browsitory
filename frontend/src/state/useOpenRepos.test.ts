import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { RepoClient } from "../ipc/RepoClient";
import { useOpenRepos } from "./useOpenRepos";
import { fakeRepoClient } from "../test/fakeRepoClient";

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient({
    openRepo: vi.fn().mockResolvedValue(undefined),
    closeRepo: vi.fn().mockResolvedValue(undefined),
    listOpenRepos: vi.fn().mockResolvedValue({ entries: [], activePath: null }),
    persistOpenRepos: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  });
}

function entry(path: string, workspaceId: string | null = null) {
  return { path, workspaceId };
}

describe("useOpenRepos", () => {
  it("restores persisted tabs on mount, with the persisted active path focused", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ entries: ["/repos/a", "/repos/b"].map((path) => entry(path)), activePath: "/repos/b" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a", "/repos/b"]);
    expect(result.current.activePath).toBe("/repos/b");
  });

  it("re-opens every persisted path against the backend on restore, not just the frontend tab list", async () => {
    // `listOpenRepos` only reports what *was* open in a prior session — the backend's worker
    // registry starts empty every launch, so a restored tab whose repo was never actually
    // (re-)opened would render with every fetch permanently failing ("repo not open").
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ entries: ["/repos/a", "/repos/b"].map((path) => entry(path)), activePath: "/repos/b" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(client.openRepo).toHaveBeenCalledWith("/repos/a");
    expect(client.openRepo).toHaveBeenCalledWith("/repos/b");
  });

  it("drops a persisted path that fails to reopen, rather than keeping a permanently broken tab", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ entries: ["/repos/a", "/repos/gone"].map((path) => entry(path)), activePath: "/repos/gone" }),
      openRepo: vi.fn().mockImplementation((path: string) =>
        path === "/repos/gone" ? Promise.reject(new Error("not a git repository")) : Promise.resolve(undefined),
      ),
    });
    const { result } = renderHook(() => useOpenRepos(client));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a"]);
    // The persisted active path failed to reopen too — falls back to the first surviving tab.
    expect(result.current.activePath).toBe("/repos/a");
  });

  it("a rejected listOpenRepos still resolves loading, rather than hanging on a blank screen", async () => {
    // `App` renders nothing while `loading` is true, so a restore that never settles is a
    // permanently blank window. Falling through to `loading: false` with no tabs lands on the
    // empty-state RepoPicker instead, with the failure surfaced via `restoreError`.
    const client = fakeClient({
      listOpenRepos: vi.fn().mockRejectedValue(new Error("config.toml is unreadable")),
    });
    const { result } = renderHook(() => useOpenRepos(client));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.openRepos).toEqual([]);
    expect(result.current.activePath).toBeNull();
    expect(result.current.restoreError).toContain("config.toml is unreadable");
  });

  it("dismissRestoreError clears restoreError", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockRejectedValue(new Error("config.toml is unreadable")),
    });
    const { result } = renderHook(() => useOpenRepos(client));

    await waitFor(() => expect(result.current.restoreError).not.toBeNull());

    act(() => result.current.dismissRestoreError());

    expect(result.current.restoreError).toBeNull();
  });

  it("opening a new path calls client.openRepo, adds a tab, and focuses it", async () => {
    const client = fakeClient();
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.openRepo("/repos/new");
    });

    expect(client.openRepo).toHaveBeenCalledWith("/repos/new");
    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/new"]);
    expect(result.current.activePath).toBe("/repos/new");
    expect(client.persistOpenRepos).toHaveBeenLastCalledWith([entry("/repos/new")], "/repos/new");
  });

  it("opening an already-open path focuses it instead of duplicating the tab", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ entries: ["/repos/a", "/repos/b"].map((path) => entry(path)), activePath: "/repos/a" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.openRepo("/repos/b");
    });

    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a", "/repos/b"]);
    expect(result.current.activePath).toBe("/repos/b");
  });

  it("closing the active tab focuses the next tab, or the previous one if it was last", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ entries: ["/repos/a", "/repos/b", "/repos/c"].map((path) => entry(path)), activePath: "/repos/c" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    act(() => result.current.closeRepo("/repos/c"));

    expect(client.closeRepo).toHaveBeenCalledWith("/repos/c");
    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a", "/repos/b"]);
    expect(result.current.activePath).toBe("/repos/b");
  });

  it("derives displayName from the path's final segment", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ entries: ["/repos/widget"].map((path) => entry(path)), activePath: "/repos/widget" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.openRepos[0].displayName).toBe("widget");
  });

  it("a failed openRepo rejects and does not add a tab or change activePath", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ entries: ["/repos/a"].map((path) => entry(path)), activePath: "/repos/a" }),
      // Rejects only the path under test — the mount-time restore of "/repos/a" also calls
      // `openRepo` now (each persisted path is re-opened for real against the fresh backend
      // worker registry) and must succeed so this test can isolate the *later* failed open.
      openRepo: vi.fn().mockImplementation((path: string) =>
        path === "/repos/broken" ? Promise.reject(new Error("not a git repository")) : Promise.resolve(undefined),
      ),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await expect(
      act(async () => {
        await result.current.openRepo("/repos/broken");
      }),
    ).rejects.toThrow("not a git repository");

    expect(client.openRepo).toHaveBeenCalledWith("/repos/broken");
    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a"]);
    expect(result.current.activePath).toBe("/repos/a");
    expect(client.persistOpenRepos).not.toHaveBeenCalled();
  });

  it("closing the middle tab of three leaves the other two's order and paths untouched, then opening a new repo only appends", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ entries: ["/repos/a", "/repos/b", "/repos/c"].map((path) => entry(path)), activePath: "/repos/a" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    act(() => result.current.closeRepo("/repos/b"));

    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a", "/repos/c"]);
    // active tab was untouched by closing a different tab
    expect(result.current.activePath).toBe("/repos/a");

    await act(async () => {
      await result.current.openRepo("/repos/d");
    });

    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a", "/repos/c", "/repos/d"]);
    expect(result.current.activePath).toBe("/repos/d");
  });

  it("closing the last open tab clears activePath, so App falls back to the RepoPicker", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ entries: ["/repos/only"].map((path) => entry(path)), activePath: "/repos/only" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    act(() => result.current.closeRepo("/repos/only"));

    expect(client.closeRepo).toHaveBeenCalledWith("/repos/only");
    expect(result.current.openRepos).toEqual([]);
    expect(result.current.activePath).toBeNull();
    expect(client.persistOpenRepos).toHaveBeenLastCalledWith([], null);
  });

  it("restores a persisted workspaceId onto its tab", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({
        entries: [entry("/repos/a", "ws-1"), entry("/repos/b")],
        activePath: "/repos/a",
      }),
    });
    const { result } = renderHook(() => useOpenRepos(client));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.openRepos[0].workspaceId).toBe("ws-1");
    expect(result.current.openRepos[1].workspaceId).toBeNull();
  });

  it("openWorkspace opens every member path, tagging each tab with the workspace id", async () => {
    const client = fakeClient();
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.openWorkspace({
        id: "ws-1",
        name: "Services",
        rootPath: "/projects",
        memberPaths: ["/projects/a", "/projects/b"],
      });
    });

    expect(client.openRepo).toHaveBeenCalledWith("/projects/a");
    expect(client.openRepo).toHaveBeenCalledWith("/projects/b");
    expect(result.current.openRepos.map((r) => [r.path, r.workspaceId])).toEqual([
      ["/projects/a", "ws-1"],
      ["/projects/b", "ws-1"],
    ]);
    expect(result.current.activePath).toBe("/projects/b");
    expect(client.persistOpenRepos).toHaveBeenLastCalledWith(
      [entry("/projects/a", "ws-1"), entry("/projects/b", "ws-1")],
      "/projects/b",
    );
  });

  it("openWorkspace skips a member that fails to open, rather than failing the whole open", async () => {
    const client = fakeClient({
      openRepo: vi.fn().mockImplementation((path: string) =>
        path === "/projects/gone" ? Promise.reject(new Error("not a git repository")) : Promise.resolve(undefined),
      ),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.openWorkspace({
        id: "ws-1",
        name: "Services",
        rootPath: "/projects",
        memberPaths: ["/projects/a", "/projects/gone"],
      });
    });

    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/projects/a"]);
  });

  it("openWorkspace tags an already-open successful member without duplicating it", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ entries: [entry("/projects/a")], activePath: "/projects/a" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.openWorkspace({
        id: "ws-1",
        name: "Services",
        rootPath: "/projects",
        memberPaths: ["/projects/a", "/projects/b"],
      });
    });

    expect(result.current.openRepos.map((r) => [r.path, r.workspaceId])).toEqual([
      ["/projects/a", "ws-1"],
      ["/projects/b", "ws-1"],
    ]);
    expect(client.persistOpenRepos).toHaveBeenLastCalledWith(
      [entry("/projects/a", "ws-1"), entry("/projects/b", "ws-1")],
      "/projects/b",
    );
  });

  it("openWorkspace keeps all members contiguous when an existing member precedes an unrelated tab", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({
        entries: [entry("/projects/a"), entry("/repos/unrelated")],
        activePath: "/repos/unrelated",
      }),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.openWorkspace({
        id: "ws-1",
        name: "Services",
        rootPath: "/projects",
        memberPaths: ["/projects/a", "/projects/b"],
      });
    });

    expect(result.current.openRepos.map((repo) => [repo.path, repo.workspaceId])).toEqual([
      ["/projects/a", "ws-1"],
      ["/projects/b", "ws-1"],
      ["/repos/unrelated", null],
    ]);
    expect(client.persistOpenRepos).toHaveBeenLastCalledWith(
      [entry("/projects/a", "ws-1"), entry("/projects/b", "ws-1"), entry("/repos/unrelated")],
      "/projects/b",
    );
  });

  describe("reopen closed tab (#613)", () => {
    async function threeTabs() {
      const client = fakeClient({
        listOpenRepos: vi.fn().mockResolvedValue({
          entries: [entry("/repos/a"), entry("/repos/b", "ws-1"), entry("/repos/c", "ws-1")],
          activePath: "/repos/a",
        }),
      });
      const hook = renderHook(() => useOpenRepos(client));
      await waitFor(() => expect(hook.result.current.loading).toBe(false));
      return { client, result: hook.result };
    }

    it("reopens a closed tab at its old position, in its workspace group, and activates it", async () => {
      const { client, result } = await threeTabs();
      expect(result.current.canReopenClosedRepo).toBe(false);

      act(() => result.current.closeRepo("/repos/b"));
      expect(result.current.canReopenClosedRepo).toBe(true);

      let failures: unknown;
      await act(async () => {
        failures = await result.current.reopenClosedRepo();
      });

      expect(failures).toEqual([]);
      expect(result.current.openRepos.map((r) => [r.path, r.workspaceId])).toEqual([
        ["/repos/a", null],
        ["/repos/b", "ws-1"],
        ["/repos/c", "ws-1"],
      ]);
      expect(result.current.activePath).toBe("/repos/b");
      expect(client.openRepo).toHaveBeenLastCalledWith("/repos/b");
      expect(client.persistOpenRepos).toHaveBeenLastCalledWith(
        [entry("/repos/a"), entry("/repos/b", "ws-1"), entry("/repos/c", "ws-1")],
        "/repos/b",
      );
      expect(result.current.canReopenClosedRepo).toBe(false);
    });

    it("walks back through closed tabs, newest first, restoring the original order", async () => {
      const { result } = await threeTabs();

      act(() => result.current.closeRepo("/repos/a"));
      act(() => result.current.closeRepo("/repos/c"));
      act(() => result.current.closeRepo("/repos/b"));
      expect(result.current.openRepos).toEqual([]);

      await act(async () => {
        await result.current.reopenClosedRepo();
      });
      expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/b"]);
      await act(async () => {
        await result.current.reopenClosedRepo();
      });
      expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/b", "/repos/c"]);
      await act(async () => {
        await result.current.reopenClosedRepo();
      });
      expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a", "/repos/b", "/repos/c"]);
      expect(result.current.activePath).toBe("/repos/a");
      expect(result.current.canReopenClosedRepo).toBe(false);
    });

    it("keeps a reopened workspace tab inside its group when its old position is taken", async () => {
      const { result } = await threeTabs();

      act(() => result.current.closeRepo("/repos/c"));
      act(() => result.current.closeRepo("/repos/a"));
      await act(async () => {
        await result.current.openRepo("/repos/d");
      });
      // [b (ws-1), d]: a goes back to index 0; c's old index 2 would land after d, outside ws-1.
      await act(async () => {
        await result.current.reopenClosedRepo();
      });
      await act(async () => {
        await result.current.reopenClosedRepo();
      });

      expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a", "/repos/b", "/repos/c", "/repos/d"]);
    });

    it("skips a closed repository that no longer opens, reports it, and reopens the one before it", async () => {
      const { client, result } = await threeTabs();
      act(() => result.current.closeRepo("/repos/b"));
      act(() => result.current.closeRepo("/repos/c"));
      vi.mocked(client.openRepo).mockImplementation((path: string) =>
        path === "/repos/c" ? Promise.reject(new Error("not a repository")) : Promise.resolve(undefined),
      );

      let failures: unknown;
      await act(async () => {
        failures = await result.current.reopenClosedRepo();
      });

      expect(failures).toEqual([{ path: "/repos/c", message: "Error: not a repository" }]);
      expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a", "/repos/b"]);
      expect(result.current.activePath).toBe("/repos/b");
      expect(result.current.canReopenClosedRepo).toBe(false);
    });

    it("remembers at most ten closed tabs", async () => {
      const paths = Array.from({ length: 12 }, (_, i) => `/repos/r${i}`);
      const client = fakeClient({
        listOpenRepos: vi.fn().mockResolvedValue({ entries: paths.map((path) => entry(path)), activePath: paths[0] }),
      });
      const { result } = renderHook(() => useOpenRepos(client));
      await waitFor(() => expect(result.current.loading).toBe(false));

      for (const path of paths) act(() => result.current.closeRepo(path));
      for (let i = 0; i < 12; i++) {
        await act(async () => {
          await result.current.reopenClosedRepo();
        });
      }

      expect(result.current.openRepos.map((r) => r.path)).toEqual(paths.slice(2));
    });
  });
});
