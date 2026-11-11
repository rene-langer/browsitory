import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

// `checkForChanges` (#650): a refresh only when the stamp differs from the one the last refresh
// read.
function stampedClient(overrides: Partial<RepoClient> = {}) {
  let stamp = "s1";
  const getChangeStamp = vi.fn(async () => stamp);
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
        return "s1";
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
