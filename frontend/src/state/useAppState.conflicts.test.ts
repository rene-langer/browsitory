import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

describe("useAppState resolveWholeFile", () => {
  it("resolves a conflicted file with one side and refreshes", async () => {
    const resolveWholeFile = vi.fn(async () => {});
    const client = transferClient({ resolveWholeFile });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(async () => {
      await result.current.resolveWholeFile("a.txt", "Ours");
    });

    expect(resolveWholeFile).toHaveBeenCalledWith(TEST_REPO_PATH, "a.txt", "Ours");
    expect(result.current.state.error).toBeNull();
  });
});

describe("useAppState markResolved", () => {
  it("passes force through and returns the outcome", async () => {
    const markResolved = vi.fn(async () => "MarkersRemain" as const);
    const client = transferClient({ markResolved });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: unknown;
    await act(async () => {
      outcome = await result.current.markResolved("a.txt", false);
    });

    expect(markResolved).toHaveBeenCalledWith(TEST_REPO_PATH, "a.txt", false);
    expect(outcome).toBe("MarkersRemain");
    expect(result.current.state.error).toBeNull();
  });

  it("returns null and shows the error when marking fails", async () => {
    const client = transferClient({
      markResolved: async () => {
        throw new Error("no conflict found for path 'a.txt'");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: unknown = "unset";
    await act(async () => {
      outcome = await result.current.markResolved("a.txt", true);
    });

    expect(outcome).toBeNull();
    expect(result.current.state.error).toBe("no conflict found for path 'a.txt'");
  });
});
