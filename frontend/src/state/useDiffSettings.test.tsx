import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { useDiffSettings, useDiffSettingsContext } from "./useDiffSettings";
import { fakeRepoClient } from "../test/fakeRepoClient";

function clientWith(overrides: Partial<RepoClient>): RepoClient {
  return fakeRepoClient({
    getDiffSettings: vi.fn(async () => ({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto" as const, algorithm: "Default" as const, showWhitespace: false, expandUntracked: true })),
    setDiffSettings: vi.fn(async () => {}),
    ...overrides,
  });
}

describe("useDiffSettings", () => {
  it("loads persisted settings", async () => {
    const client = clientWith({ getDiffSettings: vi.fn(async () => ({ ignoreWhitespace: true, contextLines: 7, syntaxHighlight: false, wordWrap: "Auto" as const, algorithm: "Default" as const, showWhitespace: false, expandUntracked: true })) });
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
      expect(client.setDiffSettings).toHaveBeenCalledWith({ ignoreWhitespace: false, contextLines: 5, syntaxHighlight: true, wordWrap: "Auto" as const, algorithm: "Default" as const, showWhitespace: false, expandUntracked: true }),
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

  it("falls back to Auto word wrap for a missing or unknown value", async () => {
    const client = clientWith({
      getDiffSettings: vi.fn(async () => ({ ignoreWhitespace: false, contextLines: 4, syntaxHighlight: true }) as never),
    });
    const { result } = renderHook(() => useDiffSettings(client));
    await waitFor(() => expect(result.current.settings.contextLines).toBe(4));
    expect(result.current.settings.wordWrap).toBe("Auto");
    act(() => result.current.update({ wordWrap: "Off", algorithm: "Default" }));
    expect(result.current.settings.wordWrap).toBe("Off");
  });

  it("loads a missing showWhitespace (an older backend) as off and persists the toggle (#616)", async () => {
    const client = clientWith({
      getDiffSettings: vi.fn(async () => ({ ignoreWhitespace: false, contextLines: 4, syntaxHighlight: true, wordWrap: "Auto", algorithm: "Default" }) as never),
    });
    const { result } = renderHook(() => useDiffSettings(client));
    await waitFor(() => expect(result.current.settings.contextLines).toBe(4));
    expect(result.current.settings.showWhitespace).toBe(false);
    act(() => result.current.update({ showWhitespace: true }));
    expect(result.current.settings.showWhitespace).toBe(true);
    await waitFor(() =>
      expect(client.setDiffSettings).toHaveBeenCalledWith({ ignoreWhitespace: false, contextLines: 4, syntaxHighlight: true, wordWrap: "Auto", algorithm: "Default", showWhitespace: true, expandUntracked: true }),
    );
  });

  it("loads a missing expandUntracked (an older backend) as on and persists turning it off (#623)", async () => {
    const client = clientWith({
      getDiffSettings: vi.fn(async () => ({ ignoreWhitespace: false, contextLines: 4, syntaxHighlight: true, wordWrap: "Auto", algorithm: "Default", showWhitespace: false }) as never),
    });
    const { result } = renderHook(() => useDiffSettings(client));
    await waitFor(() => expect(result.current.settings.contextLines).toBe(4));
    expect(result.current.settings.expandUntracked).toBe(true);
    act(() => result.current.update({ expandUntracked: false }));
    expect(result.current.settings.expandUntracked).toBe(false);
    await waitFor(() =>
      expect(client.setDiffSettings).toHaveBeenCalledWith({ ignoreWhitespace: false, contextLines: 4, syntaxHighlight: true, wordWrap: "Auto", algorithm: "Default", showWhitespace: false, expandUntracked: false }),
    );
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
    expect(result.current.settings).toEqual({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto" as const, algorithm: "Default" as const, showWhitespace: false, expandUntracked: true });
  });
});
