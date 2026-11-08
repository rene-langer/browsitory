import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { BranchInfo, RemoteInfo, RepoClient, TagInfo, WorktreeInfo } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

// Every optimistic create/rename keyed by a user-chosen name refuses a taken one before the
// optimistic update, so the list never shows it twice (#408, #413); a backend failure still
// rolls the optimistic update back.

function tag(name: string): TagInfo {
  return { name, targetId: "t", commitId: "c", annotated: false, message: null, taggerName: null, timestamp: null };
}

function remote(name: string): RemoteInfo {
  return { name, fetchUrl: `../${name}.git`, pushUrl: null, authMode: null, authUsername: null };
}

const mainWorktree: WorktreeInfo = {
  name: "main",
  path: TEST_REPO_PATH,
  head: "main",
  isMain: true,
  isLocked: false,
  isPrunable: false,
};
const linked: WorktreeInfo = {
  name: "feature",
  path: "/work/feature",
  head: "feature",
  isMain: false,
  isLocked: false,
  isPrunable: false,
};

async function loaded(overrides: Partial<RepoClient>) {
  const client = transferClient(overrides);
  const hook = renderHook(() => useAppState(client, TEST_REPO_PATH));
  await act(() => hook.result.current.refresh());
  return hook.result;
}

// Starts `call` and returns what the state looked like synchronously after it, before awaiting.
async function callAndSettle<T>(start: () => Promise<T>, inspect: () => void): Promise<T> {
  let promise!: Promise<T>;
  act(() => {
    promise = start();
  });
  inspect();
  let outcome!: T;
  await act(async () => {
    outcome = await promise;
  });
  return outcome;
}

describe("taken names (#413)", () => {
  it("createTag refuses a taken name without listing it twice or calling the backend", async () => {
    const createTag = vi.fn(async () => {});
    const result = await loaded({ listTags: async () => [tag("v1")], createTag });

    const outcome = await callAndSettle(
      () => result.current.createTag("v1", null, null),
      () => expect(result.current.state.tags.map((t) => t.name)).toEqual(["v1"]),
    );

    expect(outcome).toBe('A tag named "v1" already exists.');
    expect(result.current.state.error).toBe('A tag named "v1" already exists.');
    expect(result.current.state.tags).toEqual([tag("v1")]);
    expect(result.current.state.pending).toBe(false);
    expect(createTag).not.toHaveBeenCalled();
  });

  it("createTag refuses a name that is a folder of a taken one", async () => {
    const createTag = vi.fn(async () => {});
    const result = await loaded({ listTags: async () => [tag("v1")], createTag });

    let outcome: string | null = null;
    await act(async () => {
      outcome = await result.current.createTag("v1/rc", null, null);
    });

    expect(outcome).toBe('"v1/rc" can\'t be used while a tag named "v1" exists.');
    expect(createTag).not.toHaveBeenCalled();
  });

  it("createTag rolls the optimistic tag back when the backend refuses it", async () => {
    let rejectCreate: ((err: Error) => void) | null = null;
    const result = await loaded({
      listTags: async () => [tag("v1")],
      createTag: async () => new Promise<void>((_resolve, reject) => { rejectCreate = reject; }),
    });

    let promise!: Promise<string | null>;
    act(() => {
      promise = result.current.createTag("v2", null, null);
    });
    expect(result.current.state.tags.map((t) => t.name)).toEqual(["v1", "v2"]);

    let outcome: string | null = null;
    await act(async () => {
      rejectCreate?.(new Error("tag refused"));
      outcome = await promise;
    });
    expect(outcome).toBe("tag refused");
    expect(result.current.state.tags).toEqual([tag("v1")]);
  });

  it("createBranch refuses a taken name, and one that clashes as a folder, without calling the backend", async () => {
    const branches: BranchInfo[] = [{ name: "main", isCurrent: true }, { name: "feature", isCurrent: false }];
    const createBranch = vi.fn(async () => {});
    const result = await loaded({ listBranches: async () => branches, createBranch });

    const taken = await callAndSettle(
      () => result.current.createBranch("main", "HEAD"),
      // The current branch keeps its place (and its current marker) throughout.
      () => expect(result.current.state.branches).toEqual(branches),
    );
    expect(taken).toBe('A branch named "main" already exists.');

    let folder: string | null = null;
    await act(async () => {
      folder = await result.current.createBranch("feature/x", "HEAD");
    });
    expect(folder).toBe('"feature/x" can\'t be used while a branch named "feature" exists.');
    expect(result.current.state.branches).toEqual(branches);
    expect(createBranch).not.toHaveBeenCalled();
  });

  it("createBranch still adds a free name optimistically and keeps it on success", async () => {
    let resolveCreate: (() => void) | null = null;
    let branches: BranchInfo[] = [{ name: "main", isCurrent: true }];
    const result = await loaded({
      listBranches: async () => branches,
      createBranch: async () => new Promise<void>((resolve) => { resolveCreate = resolve; }),
    });

    const outcome = await callAndSettle(
      () => result.current.createBranch("topic", "HEAD"),
      () => {
        expect(result.current.state.branches.map((b) => b.name)).toEqual(["main", "topic"]);
        branches = [...branches, { name: "topic", isCurrent: false }];
        resolveCreate?.();
      },
    );
    expect(outcome).toBeNull();
    expect(result.current.state.branches.map((b) => b.name)).toEqual(["main", "topic"]);
  });

  it("renameBranch refuses another branch's name without calling the backend", async () => {
    const branches: BranchInfo[] = [{ name: "main", isCurrent: true }, { name: "topic", isCurrent: false }];
    const renameBranch = vi.fn(async () => {});
    const result = await loaded({ listBranches: async () => branches, renameBranch });

    await callAndSettle(
      () => result.current.renameBranch("topic", "main"),
      () => expect(result.current.state.branches).toEqual(branches),
    );

    expect(result.current.state.error).toBe('A branch named "main" already exists.');
    expect(result.current.state.branches).toEqual(branches);
    expect(renameBranch).not.toHaveBeenCalled();
  });

  it("renameBranch may move a branch into a folder named after itself", async () => {
    const renameBranch = vi.fn(async () => {});
    const result = await loaded({ listBranches: async () => [{ name: "topic", isCurrent: false }], renameBranch });

    await act(() => result.current.renameBranch("topic", "topic/old"));

    expect(renameBranch).toHaveBeenCalledWith(TEST_REPO_PATH, "topic", "topic/old");
  });

  it("renameRemote refuses another remote's name and resolves false without calling the backend", async () => {
    const renameRemote = vi.fn(async () => {});
    const result = await loaded({ listRemotes: async () => [remote("origin"), remote("upstream")], renameRemote });

    const renamed = await callAndSettle(
      () => result.current.renameRemote("upstream", "origin"),
      () => expect(result.current.state.remotes.map((r) => r.name)).toEqual(["origin", "upstream"]),
    );

    expect(renamed).toBe(false);
    expect(result.current.state.error).toBe('A remote named "origin" already exists.');
    expect(result.current.state.remotes).toEqual([remote("origin"), remote("upstream")]);
    expect(renameRemote).not.toHaveBeenCalled();
  });

  it("createWorktree refuses a linked worktree's name without calling the backend", async () => {
    const createWorktree = vi.fn(async () => {});
    const result = await loaded({ listWorktrees: async () => [mainWorktree, linked], createWorktree });

    const outcome = await callAndSettle(
      () => result.current.createWorktree("feature", "/work/other", "other", "HEAD"),
      () => expect(result.current.state.worktrees).toEqual([mainWorktree, linked]),
    );

    expect(outcome).toBe('A worktree named "feature" already exists.');
    expect(result.current.state.error).toBe('A worktree named "feature" already exists.');
    expect(createWorktree).not.toHaveBeenCalled();
  });

  it("createWorktree may use the name main, which is only the main worktree's label", async () => {
    const createWorktree = vi.fn(async () => {});
    const result = await loaded({ listWorktrees: async () => [mainWorktree], createWorktree });

    let outcome: string | null = "not set";
    await act(async () => {
      outcome = await result.current.createWorktree("main", "/work/main-copy", "copy", "HEAD");
    });

    expect(outcome).toBeNull();
    expect(createWorktree).toHaveBeenCalledOnce();
  });

  it("createWorktree leaves a listed path to the backend, without showing that folder twice meanwhile", async () => {
    let rejectCreate: ((err: Error) => void) | null = null;
    const createWorktree = vi.fn(
      async () => new Promise<void>((_resolve, reject) => { rejectCreate = reject; }),
    );
    const result = await loaded({ listWorktrees: async () => [mainWorktree, linked], createWorktree });

    // `../work/feature` from `/repo` is the listed `/work/feature`.
    const outcome = await callAndSettle(
      () => result.current.createWorktree("again", "../work/feature/", "again", "HEAD"),
      () => {
        expect(result.current.state.worktrees).toEqual([mainWorktree, linked]);
        rejectCreate?.(new Error("worktree path already exists"));
      },
    );

    expect(createWorktree).toHaveBeenCalledWith(TEST_REPO_PATH, "again", "../work/feature/", "again", "HEAD");
    expect(outcome).toBe("worktree path already exists");
    expect(result.current.state.worktrees).toEqual([mainWorktree, linked]);
  });

  it("createWorktree rolls the optimistic row back when the backend refuses a new path", async () => {
    let rejectCreate: ((err: Error) => void) | null = null;
    const result = await loaded({
      listWorktrees: async () => [mainWorktree],
      createWorktree: async () => new Promise<void>((_resolve, reject) => { rejectCreate = reject; }),
    });

    const outcome = await callAndSettle(
      () => result.current.createWorktree("wt", "/work/wt", "wt", "HEAD"),
      () => {
        expect(result.current.state.worktrees.map((w) => w.path)).toEqual([TEST_REPO_PATH, "/work/wt"]);
        rejectCreate?.(new Error("branch is checked out elsewhere"));
      },
    );

    expect(outcome).toBe("branch is checked out elsewhere");
    expect(result.current.state.worktrees).toEqual([mainWorktree]);
  });
});
