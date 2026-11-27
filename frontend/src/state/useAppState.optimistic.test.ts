import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo, StatusEntry, TagInfo, TransferProgress, WorktreeInfo } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { progress, TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

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
    expect(outcome).toBe("Couldn't add remote bad: invalid fetch URL");
    expect(result.current.state.error).toBe("Couldn't add remote bad: invalid fetch URL");
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

  it("addRemote refuses a name the list already holds without listing it twice or calling the backend (#408)", async () => {
    const origin: RemoteInfo = { name: "origin", fetchUrl: "../origin.git", pushUrl: null, authMode: null, authUsername: null };
    const addRemote = vi.fn(async () => {});
    const client = transferClient({ listRemotes: async () => [origin], addRemote });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let addPromise!: Promise<string | null>;
    act(() => {
      addPromise = result.current.addRemote("origin", "../other.git", null);
    });
    // Synchronously after the call: no optimistic duplicate.
    expect(result.current.state.remotes).toEqual([origin]);

    let outcome: string | null = null;
    await act(async () => {
      outcome = await addPromise;
    });
    expect(outcome).toBe('A remote named "origin" already exists.');
    expect(result.current.state.error).toBe('A remote named "origin" already exists.');
    expect(result.current.state.remotes).toEqual([origin]);
    expect(result.current.state.pending).toBe(false);
    expect(addRemote).not.toHaveBeenCalled();
  });

  it("addRemote rolls the optimistic remote back when the backend refuses it", async () => {
    const origin: RemoteInfo = { name: "origin", fetchUrl: "../origin.git", pushUrl: null, authMode: null, authUsername: null };
    let rejectAdd: ((err: Error) => void) | null = null;
    const client = transferClient({
      listRemotes: async () => [origin],
      addRemote: async () => new Promise<void>((_resolve, reject) => { rejectAdd = reject; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let addPromise!: Promise<string | null>;
    act(() => {
      addPromise = result.current.addRemote("upstream", "not-a-url", null);
    });
    expect(result.current.state.remotes.map((remote) => remote.name)).toEqual(["origin", "upstream"]);

    let outcome: string | null = null;
    await act(async () => {
      rejectAdd?.(new Error("invalid fetch URL"));
      outcome = await addPromise;
    });
    expect(outcome).toBe("Couldn't add remote upstream: invalid fetch URL");
    expect(result.current.state.remotes).toEqual([origin]);
    expect(result.current.state.pending).toBe(false);
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
    expect(result.current.state.error).toBe("Couldn't remove remote upstream: remove failed");
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
    expect(outcome).toBe("Couldn't create branch bad: branch already exists");
    expect(result.current.state.error).toBe("Couldn't create branch bad: branch already exists");
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
    expect(outcome).toBe("Couldn't create worktree bad: worktree path already exists");
    expect(result.current.state.error).toBe("Couldn't create worktree bad: worktree path already exists");
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
    expect(result.current.state.error).toBe("Couldn't remove worktree feature: worktree has uncommitted changes");
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
      outcome = await result.current.createTag("good", null, null);
    });
    expect(outcome).toBeNull();

    await act(async () => {
      outcome = await result.current.createTag("bad", null, null);
    });
    expect(outcome).toBe("Couldn't create tag bad: tag already exists");
    expect(result.current.state.error).toBe("Couldn't create tag bad: tag already exists");
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
      createPromise = result.current.createTag("v1.0.0", null, null);
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
    expect(result.current.state.error).toBe("Couldn't delete tag v1.0.0: delete failed");
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

    await act(() => result.current.createTag("bad", null, null));
    expect(result.current.state.error).toBe("Couldn't create tag bad: tag already exists");

    act(() => result.current.dismissError());

    expect(result.current.state.error).toBeNull();
  });
});

// #1124: Create tag and New worktree show their failure in their dialog, so the banner leaves
// it out (#996): the failure is marked as the dialog's, a taken name too.
describe("the create dialogs' failures", () => {
  it("createTag marks a failed create as shown in its dialog", async () => {
    const client = transferClient({
      listTags: async () => [],
      createTag: async () => {
        throw new Error("invalid tag name");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    let outcome: string | null = null;
    await act(async () => {
      outcome = await result.current.createTag("v1", null, null);
    });
    expect(outcome).toContain("invalid tag name");
    expect(result.current.state.errorSource).toEqual({ error: outcome, source: "dialog" });
  });

  it("createTag marks a taken name as shown in its dialog", async () => {
    const tag: TagInfo = { name: "v1", targetId: "t", commitId: "c", annotated: false, message: null, taggerName: null, timestamp: null };
    const client = transferClient({ listTags: async () => [tag] });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    let outcome: string | null = null;
    await act(async () => {
      outcome = await result.current.createTag("v1", null, null);
    });
    expect(outcome).not.toBeNull();
    expect(result.current.state.errorSource).toEqual({ error: outcome, source: "dialog" });
  });

  it("createWorktree marks a failed create as shown in its dialog", async () => {
    const client = transferClient({
      listWorktrees: async () => [],
      createWorktree: async () => {
        throw new Error("folder exists");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    let outcome: string | null = null;
    await act(async () => {
      outcome = await result.current.createWorktree("wt", "/elsewhere/wt", "wt", null);
    });
    expect(outcome).toContain("folder exists");
    expect(result.current.state.errorSource).toEqual({ error: outcome, source: "dialog" });
  });

  it("createWorktree marks a taken name as shown in its dialog", async () => {
    const worktree: WorktreeInfo = { name: "wt", path: "/elsewhere/wt", head: null, isMain: false, isLocked: false, isPrunable: false };
    const client = transferClient({ listWorktrees: async () => [worktree] });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    let outcome: string | null = null;
    await act(async () => {
      outcome = await result.current.createWorktree("wt", "/elsewhere/other", "other", null);
    });
    expect(outcome).not.toBeNull();
    expect(result.current.state.errorSource).toEqual({ error: outcome, source: "dialog" });
  });
});

// #1115: a failed stage restores the file list it changed, not the whole state of when it
// started: a fetch that completed while the stage was out stays completed, and a commit
// selected meanwhile stays selected.
describe("useAppState's failed optimistic action", () => {
  it("keeps a fetch that finished and a row selected while a stage was out", async () => {
    let listener: ((progress: TransferProgress) => void) | null = null;
    let rejectStage: ((err: Error) => void) | null = null;
    const entry: StatusEntry = { path: "a.txt", oldPath: null, staged: false, kind: "Modified" };
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      getStatus: async () => [entry],
      fetchAllRemotes: async () => "fetch-all-1",
      stageFile: () => new Promise<void>((_resolve, reject) => { rejectStage = reject; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.fetchAllRemotes());
    await act(async () => listener?.(progress("fetch-all-1", "FetchAll", "Receiving")));
    expect(result.current.state.transfer).toMatchObject({ phase: "Receiving" });

    let stage!: Promise<void>;
    act(() => {
      stage = result.current.stageFile("a.txt");
    });
    expect(result.current.state.status).toEqual([{ ...entry, staged: true }]);
    act(() => result.current.selectRow({ commitId: "abc" }));
    await act(async () => listener?.(progress("fetch-all-1", "FetchAll", "Completed")));
    expect(result.current.state.transfer).toBeNull();
    const generation = result.current.state.refreshGeneration;

    await act(async () => {
      rejectStage?.(new Error("index.lock exists"));
      await stage;
    });

    expect(result.current.state.transfer).toBeNull();
    expect(result.current.state.transferSubject).toBeNull();
    expect(result.current.state.selectedRow).toEqual({ commitId: "abc" });
    expect(result.current.state.refreshGeneration).toBeGreaterThanOrEqual(generation);
    expect(result.current.state.status).toEqual([entry]);
    expect(result.current.state.error).toContain("index.lock exists");
    expect(result.current.state.pending).toBe(false);
  });
});
