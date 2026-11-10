import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { __resetHostVisibilityForTests, publishHostVisibility } from "../ipc/hostVisibility";
import type { StatusEntry } from "../ipc/RepoClient";
import { __resetAutoFetchForTests, recordFetch } from "../lib/autoFetch";
import { fakeRepoClient } from "../test/fakeRepoClient";
import type { OpenRepo } from "./useOpenRepos";
import { useTabChangeMarkers } from "./useTabChangeMarkers";

const change: StatusEntry = { path: "file.txt", oldPath: null, staged: false, kind: "Modified" };

function tabs(...paths: string[]): OpenRepo[] {
  return paths.map((path) => ({ path, displayName: path.slice(1), alias: null, workspaceId: null }));
}

function setup(statusByPath: Record<string, StatusEntry[] | Error>) {
  const getStatus = vi.fn(async (path: string) => {
    const status = statusByPath[path] ?? [];
    if (status instanceof Error) throw status;
    return status;
  });
  const client = fakeRepoClient({ getStatus });
  const hook = renderHook(
    ({ openRepos, activePath }: { openRepos: OpenRepo[]; activePath: string | null }) =>
      useTabChangeMarkers(client, openRepos, activePath),
    { initialProps: { openRepos: tabs("/a", "/b"), activePath: "/a" } },
  );
  return { getStatus, statusByPath, ...hook };
}

afterEach(() => {
  __resetHostVisibilityForTests();
  __resetAutoFetchForTests();
});

describe("useTabChangeMarkers (#613)", () => {
  it("checks an inactive tab when it first shows up, but leaves the active one to its live status", async () => {
    const { getStatus, result } = setup({ "/b": [change] });

    await waitFor(() => expect(result.current.changedPaths).toEqual(new Set(["/b"])));
    expect(getStatus).toHaveBeenCalledTimes(1);
    expect(getStatus).toHaveBeenCalledWith("/b");

    act(() => result.current.onLiveStatus("/a", [change]));
    expect(result.current.changedPaths).toEqual(new Set(["/a", "/b"]));
    act(() => result.current.onLiveStatus("/a", []));
    expect(result.current.changedPaths).toEqual(new Set(["/b"]));
  });

  it("checks the tab the user leaves", async () => {
    const { getStatus, statusByPath, result, rerender } = setup({});
    await waitFor(() => expect(getStatus).toHaveBeenCalledWith("/b"));
    statusByPath["/a"] = [change];

    rerender({ openRepos: tabs("/a", "/b"), activePath: "/b" });

    await waitFor(() => expect(result.current.changedPaths).toEqual(new Set(["/a"])));
    expect(getStatus).toHaveBeenLastCalledWith("/a");
  });

  it("checks the inactive tabs when the window becomes visible again and after a fetch", async () => {
    const { getStatus, statusByPath, result } = setup({});
    await waitFor(() => expect(getStatus).toHaveBeenCalledTimes(1));

    act(() => publishHostVisibility(false));
    statusByPath["/b"] = [change];
    act(() => publishHostVisibility(true));
    await waitFor(() => expect(result.current.changedPaths).toEqual(new Set(["/b"])));
    expect(getStatus).toHaveBeenCalledTimes(2);

    statusByPath["/b"] = [];
    act(() => recordFetch("/a"));
    await waitFor(() => expect(result.current.changedPaths).toEqual(new Set()));
    expect(getStatus).toHaveBeenCalledTimes(3);
    expect(getStatus).not.toHaveBeenCalledWith("/a");
  });

  it("keeps the marker as it was when a check fails", async () => {
    const { getStatus, statusByPath, result } = setup({ "/b": [change] });
    await waitFor(() => expect(result.current.changedPaths).toEqual(new Set(["/b"])));

    statusByPath["/b"] = new Error("repo not open");
    act(() => recordFetch("/a"));
    await waitFor(() => expect(getStatus).toHaveBeenCalledTimes(2));
    await act(async () => {});

    expect(result.current.changedPaths).toEqual(new Set(["/b"]));
  });

  it("drops the marker of a closed tab", async () => {
    const { result, rerender } = setup({ "/b": [change] });
    await waitFor(() => expect(result.current.changedPaths).toEqual(new Set(["/b"])));

    rerender({ openRepos: tabs("/a"), activePath: "/a" });

    expect(result.current.changedPaths).toEqual(new Set());
  });
});
