import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { RemoteInfo, TagInfo, WorktreeInfo } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

describe("useAppState", () => {
  // `RemotePanel` renders the failure inline next to the Fetch URL field and keeps the typed
  // values, so it needs a per-call signal. A plain `runMutation` gives it none: it swallows the
  // error into `state.error` and always resolves.
  it("addRemote resolves to null on success and to the failure message on failure", async () => {
    const client = transferClient({
      addRemote: async (_repoPath: string, name: string) => {
        if (name === "bad") throw new Error("invalid fetch URL");
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: string | null = "not set";
    await act(async () => {
      outcome = await result.current.addRemote("good", "../good.git", null);
    });
    expect(outcome).toBeNull();

    await act(async () => {
      outcome = await result.current.addRemote("bad", "not-a-url", null);
    });
    // No leaked `"Error: "` prefix — this string is rendered to the user verbatim.
    expect(outcome).toBe("invalid fetch URL");
    expect(result.current.state.error).toBe("invalid fetch URL");
    expect(result.current.state.pending).toBe(false);
  });

  it("addRemote adds the remote to state before the backend call resolves", async () => {
    let resolveAdd: (() => void) | null = null;
    const client = transferClient({
      listRemotes: async () => [],
      addRemote: async () => new Promise<void>((resolve) => { resolveAdd = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let addPromise!: Promise<string | null>;
    act(() => {
      addPromise = result.current.addRemote("upstream", "../upstream.git", null);
    });

    expect(result.current.state.remotes).toEqual([
      { name: "upstream", fetchUrl: "../upstream.git", pushUrl: null, authMode: null, authUsername: null },
    ]);

    await act(async () => {
      resolveAdd?.();
      await addPromise;
    });
  });

  it("removeRemote removes the remote from state before the backend call resolves, restores it on failure", async () => {
    const remoteA: RemoteInfo = { name: "upstream", fetchUrl: "../upstream.git", pushUrl: null, authMode: null, authUsername: null };
    const client = transferClient({
      listRemotes: async () => [remoteA],
      removeRemote: async () => {
        throw new Error("remove failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.removeRemote("upstream", false));

    expect(result.current.state.remotes).toEqual([remoteA]);
    expect(result.current.state.error).toBe("remove failed");
  });

  it("renameRemote renames the remote in state before the backend call resolves, resolves true on success", async () => {
    const remoteA: RemoteInfo = { name: "old", fetchUrl: "../r.git", pushUrl: null, authMode: null, authUsername: null };
    let resolveRename: (() => void) | null = null;
    const client = transferClient({
      listRemotes: async () => [remoteA],
      renameRemote: async () => new Promise<void>((resolve) => { resolveRename = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let renamePromise!: Promise<boolean>;
    act(() => {
      renamePromise = result.current.renameRemote("old", "new");
    });

    expect(result.current.state.remotes[0].name).toBe("new");

    let succeeded!: boolean;
    await act(async () => {
      resolveRename?.();
      succeeded = await renamePromise;
    });
    expect(succeeded).toBe(true);
  });

  it("renameRemote resolves false and restores the original name on failure", async () => {
    const remoteA: RemoteInfo = { name: "old", fetchUrl: "../r.git", pushUrl: null, authMode: null, authUsername: null };
    const client = transferClient({
      listRemotes: async () => [remoteA],
      renameRemote: async () => {
        throw new Error("rename failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    const succeeded = await act(() => result.current.renameRemote("old", "new"));

    expect(succeeded).toBe(false);
    expect(result.current.state.remotes).toEqual([remoteA]);
  });

  it("updateRemoteUrls updates the URLs in state before the backend call resolves", async () => {
    const remoteA: RemoteInfo = { name: "origin", fetchUrl: "../old.git", pushUrl: null, authMode: null, authUsername: null };
    let resolveUpdate: (() => void) | null = null;
    const client = transferClient({
      listRemotes: async () => [remoteA],
      updateRemoteUrls: async () => new Promise<void>((resolve) => { resolveUpdate = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let updatePromise!: Promise<void>;
    act(() => {
      updatePromise = result.current.updateRemoteUrls("origin", "../new.git", "../push.git");
    });

    expect(result.current.state.remotes[0].fetchUrl).toBe("../new.git");
    expect(result.current.state.remotes[0].pushUrl).toBe("../push.git");

    await act(async () => {
      resolveUpdate?.();
      await updatePromise;
    });
  });

  // Same `Promise<string | null>` contract as `addRemote` above, for the same reason:
  // `BranchSwitcher`, `WorktreePanel`, and `TagPanel` each render their create-form's failure
  // inline next to the form rather than routing it through the shared banner (issue #30/UX-002).
  it("createBranch resolves to null on success and to the failure message on failure", async () => {
    const client = transferClient({
      createBranch: async (_repoPath, name) => {
        if (name === "bad") throw new Error("branch already exists");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: string | null = "not set";
    await act(async () => {
      outcome = await result.current.createBranch("good", "HEAD");
    });
    expect(outcome).toBeNull();

    await act(async () => {
      outcome = await result.current.createBranch("bad", "HEAD");
    });
    expect(outcome).toBe("branch already exists");
    expect(result.current.state.error).toBe("branch already exists");
  });

  it("createWorktree resolves to null on success and to the failure message on failure", async () => {
    const client = transferClient({
      createWorktree: async (_repoPath, name) => {
        if (name === "bad") throw new Error("worktree path already exists");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: string | null = "not set";
    await act(async () => {
      outcome = await result.current.createWorktree("good", "/repo-good", "good", null);
    });
    expect(outcome).toBeNull();

    await act(async () => {
      outcome = await result.current.createWorktree("bad", "/repo-bad", "bad", null);
    });
    expect(outcome).toBe("worktree path already exists");
    expect(result.current.state.error).toBe("worktree path already exists");
  });

  it("createWorktree adds the worktree to state before the backend call resolves", async () => {
    let resolveCreate: (() => void) | null = null;
    const client = transferClient({
      listWorktrees: async () => [],
      createWorktree: async () => new Promise<void>((resolve) => { resolveCreate = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let createPromise!: Promise<string | null>;
    act(() => {
      createPromise = result.current.createWorktree("feature", "/repo/../feature", "feature-branch", null);
    });

    expect(result.current.state.worktrees).toEqual([
      { name: "feature", path: "/repo/../feature", head: null, isMain: false, isLocked: false, isPrunable: false },
    ]);

    await act(async () => {
      resolveCreate?.();
      await createPromise;
    });
  });

  it("removeWorktree removes the worktree from state before the backend call resolves, restores it on failure", async () => {
    const worktreeA: WorktreeInfo = { name: "feature", path: "/repo/../feature", head: "abc", isMain: false, isLocked: false, isPrunable: false };
    const client = transferClient({
      listWorktrees: async () => [worktreeA],
      removeWorktree: async () => {
        throw new Error("worktree has uncommitted changes");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.removeWorktree("feature"));

    expect(result.current.state.worktrees).toEqual([worktreeA]);
    expect(result.current.state.error).toBe("worktree has uncommitted changes");
  });

  it("createTag resolves to null on success and to the failure message on failure", async () => {
    const client = transferClient({
      createTag: async (_repoPath, name) => {
        if (name === "bad") throw new Error("tag already exists");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: string | null = "not set";
    await act(async () => {
      outcome = await result.current.createTag("good", null);
    });
    expect(outcome).toBeNull();

    await act(async () => {
      outcome = await result.current.createTag("bad", null);
    });
    expect(outcome).toBe("tag already exists");
    expect(result.current.state.error).toBe("tag already exists");
  });

  it("createTag adds the tag to state before the backend call resolves", async () => {
    let resolveCreate: (() => void) | null = null;
    const client = transferClient({
      listTags: async () => [],
      createTag: async () => new Promise<void>((resolve) => { resolveCreate = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let createPromise!: Promise<string | null>;
    act(() => {
      createPromise = result.current.createTag("v1.0.0", null);
    });

    expect(result.current.state.tags).toHaveLength(1);
    expect(result.current.state.tags[0].name).toBe("v1.0.0");
    expect(result.current.state.tags[0].annotated).toBe(false);

    await act(async () => {
      resolveCreate?.();
      await createPromise;
    });
  });

  it("deleteTag removes the tag from state before the backend call resolves, restores it on failure", async () => {
    const tagA: TagInfo = { name: "v1.0.0", targetId: "abc", commitId: "abc", annotated: false, message: null, taggerName: null, timestamp: null };
    const client = transferClient({
      listTags: async () => [tagA],
      deleteTag: async () => {
        throw new Error("delete failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.deleteTag("v1.0.0"));

    expect(result.current.state.tags).toEqual([tagA]);
    expect(result.current.state.error).toBe("delete failed");
  });

  // The global banner's dismiss control (issue #30/UX-002): without it, `state.error` only ever
  // clears on the next successful action of the same kind, so a user who moves on to a different
  // panel is left with a stale-looking banner indefinitely.
  it("dismissError clears state.error without waiting for another action", async () => {
    const client = transferClient({
      createTag: async () => {
        throw new Error("tag already exists");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.createTag("bad", null));
    expect(result.current.state.error).toBe("tag already exists");

    act(() => result.current.dismissError());

    expect(result.current.state.error).toBeNull();
  });
});
