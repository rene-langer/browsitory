import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { RepoClient, Workspace } from "../ipc/RepoClient";
import { useWorkspaces } from "./useWorkspaces";

const workspace: Workspace = {
  id: "ws-1",
  name: "Services",
  rootPath: "/projects",
  memberPaths: ["/projects/a", "/projects/b"],
};

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return {
    listWorkspaces: vi.fn().mockResolvedValue([]),
    saveWorkspace: vi.fn().mockResolvedValue("ws-new"),
    updateWorkspace: vi.fn().mockResolvedValue(undefined),
    deleteWorkspace: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  } as unknown as RepoClient;
}

describe("useWorkspaces", () => {
  it("loads the workspace list on mount", async () => {
    const client = fakeClient({ listWorkspaces: vi.fn().mockResolvedValue([workspace]) });
    const { result } = renderHook(() => useWorkspaces(client));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.workspaces).toEqual([workspace]);
    expect(result.current.error).toBeNull();
  });

  it("surfaces a rejected listWorkspaces as error, with loading still resolving", async () => {
    const client = fakeClient({ listWorkspaces: vi.fn().mockRejectedValue(new Error("config unreadable")) });
    const { result } = renderHook(() => useWorkspaces(client));

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.workspaces).toEqual([]);
    expect(result.current.error).toContain("config unreadable");
  });

  it("createWorkspace calls saveWorkspace and refreshes the list", async () => {
    const client = fakeClient({
      saveWorkspace: vi.fn().mockResolvedValue("ws-new"),
      listWorkspaces: vi
        .fn()
        .mockResolvedValueOnce([])
        .mockResolvedValueOnce([workspace]),
    });
    const { result } = renderHook(() => useWorkspaces(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    let id: string | undefined;
    await act(async () => {
      id = await result.current.createWorkspace("Services", "/projects", ["/projects/a", "/projects/b"]);
    });

    expect(id).toBe("ws-new");
    expect(client.saveWorkspace).toHaveBeenCalledWith("Services", "/projects", ["/projects/a", "/projects/b"]);
    expect(result.current.workspaces).toEqual([workspace]);
  });

  it("editWorkspace calls updateWorkspace and refreshes the list", async () => {
    const client = fakeClient({
      listWorkspaces: vi
        .fn()
        .mockResolvedValueOnce([workspace])
        .mockResolvedValueOnce([{ ...workspace, name: "Renamed" }]),
    });
    const { result } = renderHook(() => useWorkspaces(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.editWorkspace("ws-1", "Renamed", ["/projects/a"]);
    });

    expect(client.updateWorkspace).toHaveBeenCalledWith("ws-1", "Renamed", ["/projects/a"]);
    expect(result.current.workspaces[0].name).toBe("Renamed");
  });

  it("deleteWorkspace calls deleteWorkspace and refreshes the list", async () => {
    const client = fakeClient({
      listWorkspaces: vi.fn().mockResolvedValueOnce([workspace]).mockResolvedValueOnce([]),
    });
    const { result } = renderHook(() => useWorkspaces(client));
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.deleteWorkspace("ws-1");
    });

    expect(client.deleteWorkspace).toHaveBeenCalledWith("ws-1");
    expect(result.current.workspaces).toEqual([]);
  });
});
