import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { useDiffSettings, useDiffSettingsContext } from "./useDiffSettings";

function clientWith(overrides: Partial<RepoClient>): RepoClient {
  return {
    getDiffSettings: vi.fn(async () => ({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true })),
    setDiffSettings: vi.fn(async () => {}),
    ...overrides,
  } as unknown as RepoClient; // only the two settings methods are used by this hook
}

describe("useDiffSettings", () => {
  it("loads persisted settings", async () => {
    const client = clientWith({ getDiffSettings: vi.fn(async () => ({ ignoreWhitespace: true, contextLines: 7, syntaxHighlight: false })) });
    const { result } = renderHook(() => useDiffSettings(client));
    await waitFor(() => expect(result.current.settings.contextLines).toBe(7));
    expect(result.current.settings.ignoreWhitespace).toBe(true);
  });

  it("updates optimistically and persists the merged value", async () => {
    const client = clientWith({});
    const { result } = renderHook(() => useDiffSettings(client));
    await waitFor(() => expect(client.getDiffSettings).toHaveBeenCalled());
    act(() => result.current.update({ contextLines: 5 }));
    expect(result.current.settings.contextLines).toBe(5);
    await waitFor(() =>
      expect(client.setDiffSettings).toHaveBeenCalledWith({ ignoreWhitespace: false, contextLines: 5, syntaxHighlight: true }),
    );
  });

  it("clamps context lines to 0..20", async () => {
    const client = clientWith({});
    const { result } = renderHook(() => useDiffSettings(client));
    act(() => result.current.update({ contextLines: 50 }));
    expect(result.current.settings.contextLines).toBe(20);
    act(() => result.current.update({ contextLines: -1 }));
    expect(result.current.settings.contextLines).toBe(0);
  });

  it("reverts and reports when persisting fails", async () => {
    const client = clientWith({ setDiffSettings: vi.fn(async () => { throw new Error("disk full"); }) });
    const { result } = renderHook(() => useDiffSettings(client));
    await waitFor(() => expect(client.getDiffSettings).toHaveBeenCalled());
    act(() => result.current.update({ ignoreWhitespace: true }));
    await waitFor(() => expect(result.current.error).toContain("disk full"));
    expect(result.current.settings.ignoreWhitespace).toBe(false);
  });

  it("keeps defaults when getDiffSettings throws synchronously", () => {
    const client = clientWith({ getDiffSettings: (() => { throw new Error("unimplemented"); }) as RepoClient["getDiffSettings"] });
    const { result } = renderHook(() => useDiffSettings(client));
    expect(result.current.settings.contextLines).toBe(3);
  });

  it("falls back to defaults without a provider", () => {
    const { result } = renderHook(() => useDiffSettingsContext());
    expect(result.current.settings).toEqual({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true });
  });
});
