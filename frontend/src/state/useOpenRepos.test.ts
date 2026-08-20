import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { RepoClient } from "../ipc/RepoClient";
import { useOpenRepos } from "./useOpenRepos";

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return {
    openRepo: vi.fn().mockResolvedValue(undefined),
    closeRepo: vi.fn().mockResolvedValue(undefined),
    listOpenRepos: vi.fn().mockResolvedValue({ paths: [], activePath: null }),
    persistOpenRepos: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  } as unknown as RepoClient;
}

describe("useOpenRepos", () => {
  it("restores persisted tabs on mount, with the persisted active path focused", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ paths: ["/repos/a", "/repos/b"], activePath: "/repos/b" }),
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
      listOpenRepos: vi.fn().mockResolvedValue({ paths: ["/repos/a", "/repos/b"], activePath: "/repos/b" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(client.openRepo).toHaveBeenCalledWith("/repos/a");
    expect(client.openRepo).toHaveBeenCalledWith("/repos/b");
  });

  it("drops a persisted path that fails to reopen, rather than keeping a permanently broken tab", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ paths: ["/repos/a", "/repos/gone"], activePath: "/repos/gone" }),
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
    expect(client.persistOpenRepos).toHaveBeenLastCalledWith(["/repos/new"], "/repos/new");
  });

  it("opening an already-open path focuses it instead of duplicating the tab", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ paths: ["/repos/a", "/repos/b"], activePath: "/repos/a" }),
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
      listOpenRepos: vi.fn().mockResolvedValue({ paths: ["/repos/a", "/repos/b", "/repos/c"], activePath: "/repos/c" }),
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
      listOpenRepos: vi.fn().mockResolvedValue({ paths: ["/repos/widget"], activePath: "/repos/widget" }),
    });
    const { result } = renderHook(() => useOpenRepos(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.openRepos[0].displayName).toBe("widget");
  });

  it("a failed openRepo rejects and does not add a tab or change activePath", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ paths: ["/repos/a"], activePath: "/repos/a" }),
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

    expect(result.current.openRepos.map((r) => r.path)).toEqual(["/repos/a"]);
    expect(result.current.activePath).toBe("/repos/a");
    expect(client.persistOpenRepos).not.toHaveBeenCalled();
  });

  it("closing the middle tab of three leaves the other two's order and paths untouched, then opening a new repo only appends", async () => {
    const client = fakeClient({
      listOpenRepos: vi.fn().mockResolvedValue({ paths: ["/repos/a", "/repos/b", "/repos/c"], activePath: "/repos/a" }),
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
  });
});
