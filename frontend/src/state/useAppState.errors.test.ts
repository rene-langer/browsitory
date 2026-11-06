import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

// #177: errors survive refreshes. A refresh clears only the error a failed refresh set; a
// mutation clears any error when it starts.
describe("useAppState error lifecycle", () => {
  it("a refresh keeps an error a mutation set", async () => {
    const client = transferClient({
      stageFile: async () => {
        throw new Error("boom");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.stageFile("a.txt"));
    expect(result.current.state.error).toBe("boom");
    await act(() => result.current.refresh());
    expect(result.current.state.error).toBe("boom");
  });

  it("a successful refresh clears the error a failed refresh set", async () => {
    let failStatus = true;
    const client = transferClient({
      getStatus: async () => {
        if (failStatus) {
          failStatus = false;
          throw new Error("status failed");
        }
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.error).toBe("status failed");
    await act(() => result.current.refresh());
    expect(result.current.state.error).toBeNull();
  });

  it("a failed refresh replaces a mutation's error, and a successful refresh then clears it", async () => {
    let failStatus = false;
    const client = transferClient({
      stageFile: async () => {
        throw new Error("boom");
      },
      getStatus: async () => {
        if (failStatus) {
          failStatus = false;
          throw new Error("worker stopped");
        }
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.stageFile("a.txt"));
    expect(result.current.state.error).toBe("boom");
    failStatus = true;
    await act(() => result.current.refresh());
    expect(result.current.state.error).toBe("worker stopped");
    await act(() => result.current.refresh());
    expect(result.current.state.error).toBeNull();
  });

  // m3: a read remembers the text of the error it set only while that error is on screen.
  it.each([
    ["after it is dismissed", "dismiss"],
    ["after a mutation starts", "mutation"],
  ])("a refresh no longer owns its error text %s", async (_label, how) => {
    let failStatus = true;
    let stageCalls = 0;
    const client = transferClient({
      getStatus: async () => {
        if (failStatus) {
          failStatus = false;
          throw new Error("worker stopped");
        }
        return [];
      },
      stageFile: async () => {
        stageCalls += 1;
        throw new Error("worker stopped");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.error).toBe("worker stopped");
    if (how === "dismiss") act(() => result.current.dismissError());
    // The mutation clears the error when it starts, then fails with the same text.
    await act(() => result.current.stageFile("a.txt"));
    expect(stageCalls).toBe(1);
    expect(result.current.state.error).toBe("worker stopped");

    await act(() => result.current.refresh());

    expect(result.current.state.error).toBe("worker stopped");
  });

  it("selecting a reflog reference keeps another action's error", async () => {
    const client = transferClient({
      stageFile: async () => {
        throw new Error("boom");
      },
      getReflog: async () => [],
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.stageFile("a.txt"));
    await act(() => result.current.selectReflogReference("HEAD"));
    expect(result.current.state.error).toBe("boom");
  });

  it("a reflog read that failed and then succeeds clears its own error", async () => {
    let fail = true;
    const client = transferClient({
      getReflog: async () => {
        if (fail) {
          fail = false;
          throw new Error("reflog failed");
        }
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.selectReflogReference("HEAD"));
    expect(result.current.state.error).toContain("reflog failed");
    await act(() => result.current.selectReflogReference("HEAD"));
    expect(result.current.state.error).toBeNull();
  });

  it("listing pull requests keeps another action's error", async () => {
    const client = transferClient({
      stageFile: async () => {
        throw new Error("boom");
      },
      listPullRequests: async () => ({ pullRequests: [], truncated: false }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.stageFile("a.txt"));
    await act(() => result.current.listPullRequests("origin", "me"));
    expect(result.current.state.error).toBe("boom");
  });

  it("a pull request list that failed and then succeeds clears its own error", async () => {
    let fail = true;
    const client = transferClient({
      listPullRequests: async () => {
        if (fail) {
          fail = false;
          throw new Error("forge down");
        }
        return { pullRequests: [], truncated: false };
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.listPullRequests("origin", "me"));
    expect(result.current.state.error).toBe("forge down");
    await act(() => result.current.listPullRequests("origin", "me"));
    expect(result.current.state.error).toBeNull();
  });

  it("a refresh that overlaps a failing mutation keeps that mutation's error (the VSCode Cancel case)", async () => {
    let releaseStatus: () => void = () => {};
    let holdStatus = false;
    const client = transferClient({
      getStatus: () =>
        holdStatus
          ? new Promise((resolve) => {
              releaseStatus = () => resolve([]);
            })
          : Promise.resolve([]),
      stageFile: async () => {
        throw new Error("The pre-commit hook was cancelled.");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    holdStatus = true;
    let refreshing!: Promise<void>;
    act(() => {
      refreshing = result.current.refresh();
    });
    holdStatus = false;
    await act(() => result.current.stageFile("a.txt"));
    expect(result.current.state.error).toBe("The pre-commit hook was cancelled.");
    await act(async () => {
      releaseStatus();
      await refreshing;
    });
    expect(result.current.state.error).toBe("The pre-commit hook was cancelled.");
  });
});
