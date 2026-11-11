import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";
import { graphPage } from "../test/fakeRepoClient";

// `checkForChanges` (#650): a refresh only when the stamp differs from the one the last refresh
// read.
function stampedClient(overrides: Partial<RepoClient> = {}) {
  let stamp = "s1";
  const getChangeStamp = vi.fn(async () => ({ refs: stamp, workTree: stamp }));
  const getStatus = vi.fn(async () => []);
  const client = transferClient({ getChangeStamp, getStatus, ...overrides });
  return { client, getChangeStamp, getStatus, setStamp: (next: string) => (stamp = next) };
}

describe("useAppState checkForChanges", () => {
  it("does nothing before the first refresh", async () => {
    const { client, getStatus } = stampedClient();
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {
      await result.current.checkForChanges();
    });
    expect(getStatus).not.toHaveBeenCalled();
  });

  it("refreshes only when the stamp changed since the last refresh", async () => {
    const { client, getStatus, setStamp } = stampedClient();
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {
      await result.current.refresh();
    });
    expect(getStatus).toHaveBeenCalledTimes(1);

    await act(async () => {
      await result.current.checkForChanges();
    });
    expect(getStatus).toHaveBeenCalledTimes(1);

    setStamp("s2");
    const generation = result.current.state.refreshGeneration;
    await act(async () => {
      await result.current.checkForChanges();
    });
    expect(getStatus).toHaveBeenCalledTimes(2);
    expect(result.current.state.refreshGeneration).toBe(generation + 1);

    // That refresh read "s2": no second one.
    await act(async () => {
      await result.current.checkForChanges();
    });
    expect(getStatus).toHaveBeenCalledTimes(2);
  });

  it("doesn't refresh after the app's own change was already refreshed", async () => {
    const { client, getStatus, setStamp } = stampedClient();
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {
      await result.current.refresh();
    });
    // An action of the app changes the repository and refreshes (as `runMutation` does).
    setStamp("after-own-action");
    await act(async () => {
      await result.current.refresh();
    });
    await act(async () => {
      await result.current.checkForChanges();
    });
    expect(getStatus).toHaveBeenCalledTimes(2);
  });

  it("skips the refresh when it is no longer wanted once the stamp arrives", async () => {
    const { client, getStatus, setStamp } = stampedClient();
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {
      await result.current.refresh();
    });
    setStamp("s2");
    await act(async () => {
      await result.current.checkForChanges(() => false);
    });
    expect(getStatus).toHaveBeenCalledTimes(1);
  });

  it("doesn't check while a refresh runs", async () => {
    const { client, getChangeStamp, getStatus, setStamp } = stampedClient();
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {
      await result.current.refresh();
    });
    setStamp("s2");
    let pending: Promise<void> = Promise.resolve();
    await act(async () => {
      pending = result.current.refresh();
      await result.current.checkForChanges();
      await pending;
    });
    // One stamp per refresh, none for the check.
    expect(getChangeStamp).toHaveBeenCalledTimes(2);
    expect(getStatus).toHaveBeenCalledTimes(2);
  });

  it("stays quiet when the stamp can't be read", async () => {
    const { client, getStatus } = stampedClient({
      getChangeStamp: async () => {
        throw new Error("repo not open");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {
      await result.current.refresh();
    });
    expect(result.current.state.error).toBeNull();
    await act(async () => {
      await result.current.checkForChanges();
    });
    expect(getStatus).toHaveBeenCalledTimes(1);
    expect(result.current.state.error).toBeNull();
  });

  it("refreshes once after a refresh that couldn't read the stamp, when it can be read again", async () => {
    let fail = true;
    const { client, getStatus } = stampedClient({
      getChangeStamp: async () => {
        if (fail) throw new Error("busy");
        return { refs: "s1", workTree: "s1" };
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {
      await result.current.refresh();
    });
    fail = false;
    await act(async () => {
      await result.current.checkForChanges();
    });
    expect(getStatus).toHaveBeenCalledTimes(2);
    await act(async () => {
      await result.current.checkForChanges();
    });
    expect(getStatus).toHaveBeenCalledTimes(2);
  });

  it("keeps the stamp of a failed refresh, so checks don't retry it every time", async () => {
    let failStatus = true;
    const getStatus = vi.fn(async () => {
      if (failStatus) throw new Error("status failed");
      return [];
    });
    const { client } = stampedClient({ getStatus });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {
      await result.current.refresh();
    });
    expect(result.current.state.error).not.toBeNull();
    failStatus = false;
    await act(async () => {
      await result.current.checkForChanges();
    });
    expect(getStatus).toHaveBeenCalledTimes(1);
  });
});

// #685: the history reads (graph, branches, tags, upstreams) run only when the stamp's `refs`
// part, the graph selection or the limit changed since they were read.
describe("useAppState refresh scope", () => {
  function scopedClient(overrides: Partial<RepoClient> = {}) {
    const stamp = { refs: "r1", workTree: "w1" };
    const reads = {
      getStatus: vi.fn(async () => []),
      getCommitGraph: vi.fn(async () => graphPage([])),
      listBranches: vi.fn(async () => []),
      listTags: vi.fn(async () => []),
      getRemoteUpstreams: vi.fn(async () => []),
    };
    const client = transferClient({ getChangeStamp: async () => ({ ...stamp }), ...reads, ...overrides });
    return { client, stamp, reads };
  }
  const historyReads = (reads: ReturnType<typeof scopedClient>["reads"]) => [
    reads.getCommitGraph.mock.calls.length,
    reads.listBranches.mock.calls.length,
    reads.listTags.mock.calls.length,
    reads.getRemoteUpstreams.mock.calls.length,
  ];

  it("keeps the history when only the work tree part changed", async () => {
    const { client, stamp, reads } = scopedClient();
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {
      await result.current.refresh();
    });
    expect(historyReads(reads)).toEqual([1, 1, 1, 1]);

    stamp.workTree = "w2";
    await act(async () => {
      await result.current.refresh();
    });
    expect(reads.getStatus).toHaveBeenCalledTimes(2);
    expect(historyReads(reads)).toEqual([1, 1, 1, 1]);
    expect(result.current.state.refreshGeneration).toBe(2);
  });

  it("reads the history again when the refs part changed", async () => {
    const { client, stamp, reads } = scopedClient();
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {
      await result.current.refresh();
    });
    stamp.refs = "r2";
    await act(async () => {
      await result.current.refresh();
    });
    expect(historyReads(reads)).toEqual([2, 2, 2, 2]);
  });

  it("reads the history again for a new selection and the Refresh command", async () => {
    let local: string[] | null = null;
    const { client, reads } = scopedClient({ getGraphBranchSelection: async () => local });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {
      await result.current.refresh();
    });
    local = ["main"];
    await act(async () => {
      await result.current.refresh();
    });
    expect(reads.getCommitGraph).toHaveBeenCalledTimes(2);
    await act(async () => {
      await result.current.refresh({ full: true });
    });
    expect(historyReads(reads)).toEqual([3, 3, 3, 3]);
  });

  it("reads everything after a failed refresh and without a stamp", async () => {
    let failTags = true;
    let stampFails = false;
    const { client, stamp, reads } = scopedClient({
      listTags: vi.fn(async () => {
        if (failTags) throw new Error("tags failed");
        return [];
      }),
    });
    const stamped = client.getChangeStamp;
    client.getChangeStamp = async (path) => {
      if (stampFails) throw new Error("busy");
      return stamped(path);
    };
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {
      await result.current.refresh();
    });
    expect(result.current.state.error).not.toBeNull();
    failTags = false;
    await act(async () => {
      await result.current.refresh();
    });
    expect(reads.getCommitGraph).toHaveBeenCalledTimes(2);
    stampFails = true;
    await act(async () => {
      await result.current.refresh();
    });
    expect(reads.getCommitGraph).toHaveBeenCalledTimes(3);
    expect(stamp.refs).toBe("r1");
  });

  it("joins a Refresh asked for while a refresh waits", async () => {
    let answerFirst: (() => void) | null = null;
    let statusReads = 0;
    const { client, reads } = scopedClient({
      getStatus: async () => {
        statusReads += 1;
        if (statusReads === 1) await new Promise<void>((resolve) => (answerFirst = resolve));
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    let first!: Promise<void>;
    act(() => {
      first = result.current.refresh();
    });
    await act(async () => {
      while (answerFirst === null) await Promise.resolve();
    });
    let waiting!: Promise<void>;
    act(() => {
      void result.current.refresh();
      waiting = result.current.refresh({ full: true });
    });
    await act(async () => {
      answerFirst?.();
      await Promise.all([first, waiting]);
    });
    expect(statusReads).toBe(2);
    expect(reads.getCommitGraph).toHaveBeenCalledTimes(2);
  });
});
