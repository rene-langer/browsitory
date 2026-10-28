import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { useCommitMessageSettings, useCommitMessageSettingsContext } from "./useCommitMessageSettings";

function clientWith(overrides: Partial<RepoClient>): RepoClient {
  return {
    getCommitMessageSettings: vi.fn(async () => ({ summaryLimit: 50, bodyRuler: 72, showRuler: true })),
    setCommitMessageSettings: vi.fn(async () => {}),
    ...overrides,
  } as unknown as RepoClient; // only the two settings methods are used by this hook
}

describe("useCommitMessageSettings", () => {
  it("loads persisted settings once, with defaults until then", async () => {
    const client = clientWith({
      getCommitMessageSettings: vi.fn(async () => ({ summaryLimit: 65, bodyRuler: 80, showRuler: false })),
    });
    const { result, rerender } = renderHook(() => useCommitMessageSettings(client));
    expect(result.current.settings).toEqual({ summaryLimit: 50, bodyRuler: 72, showRuler: true });
    await waitFor(() => expect(result.current.settings.summaryLimit).toBe(65));
    rerender();
    expect(client.getCommitMessageSettings).toHaveBeenCalledTimes(1);
  });

  it("updates optimistically, clamps to 10..200 and persists the merged value", async () => {
    const client = clientWith({});
    const { result } = renderHook(() => useCommitMessageSettings(client));
    await waitFor(() => expect(client.getCommitMessageSettings).toHaveBeenCalled());
    act(() => result.current.update({ bodyRuler: 500 }));
    expect(result.current.settings.bodyRuler).toBe(200);
    act(() => result.current.update({ summaryLimit: 3.6 }));
    expect(result.current.settings.summaryLimit).toBe(10);
    await waitFor(() =>
      expect(client.setCommitMessageSettings).toHaveBeenLastCalledWith({ summaryLimit: 10, bodyRuler: 200, showRuler: true }),
    );
  });

  it("reverts and reports when persisting fails", async () => {
    const client = clientWith({ setCommitMessageSettings: vi.fn(async () => { throw new Error("disk full"); }) });
    const { result } = renderHook(() => useCommitMessageSettings(client));
    await waitFor(() => expect(client.getCommitMessageSettings).toHaveBeenCalled());
    act(() => result.current.update({ showRuler: false }));
    await waitFor(() => expect(result.current.error).toContain("disk full"));
    expect(result.current.settings.showRuler).toBe(true);
  });

  it("keeps defaults when loading fails", async () => {
    const client = clientWith({ getCommitMessageSettings: vi.fn(async () => { throw new Error("no config"); }) });
    const { result } = renderHook(() => useCommitMessageSettings(client));
    await waitFor(() => expect(client.getCommitMessageSettings).toHaveBeenCalled());
    expect(result.current.settings).toEqual({ summaryLimit: 50, bodyRuler: 72, showRuler: true });
    expect(result.current.error).toBeNull();
  });

  it("falls back to defaults without a provider", () => {
    const { result } = renderHook(() => useCommitMessageSettingsContext());
    expect(result.current.settings).toEqual({ summaryLimit: 50, bodyRuler: 72, showRuler: true });
  });
});
