import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readCommitDraft } from "../lib/commitDraftStorage";
import { useCommitDrafts } from "./useCommitDrafts";

// #1143: a draft survives the app quitting, the VSCode panel closing and the window's Reload.
describe("useCommitDrafts", () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => vi.useRealTimers());

  it("stores a draft shortly after it changes", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useCommitDrafts());
    act(() => result.current.onCommitDraftChange("/repos/a", { message: "w", lastSeeded: "" }));
    act(() => result.current.onCommitDraftChange("/repos/a", { message: "wip", lastSeeded: "" }));
    act(() => vi.advanceTimersByTime(1000));
    expect(readCommitDraft("/repos/a")).toMatchObject({ message: "wip" });
  });

  it("stores a draft still waiting when the page goes away", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useCommitDrafts());
    act(() => result.current.onCommitDraftChange("/repos/a", { message: "wip", lastSeeded: "" }));
    act(() => {
      window.dispatchEvent(new Event("pagehide"));
    });
    expect(readCommitDraft("/repos/a")).toMatchObject({ message: "wip" });
  });

  it("stores a draft still waiting when it unmounts, and a new mount reads it", () => {
    vi.useFakeTimers();
    const first = renderHook(() => useCommitDrafts());
    act(() => first.result.current.onCommitDraftChange("/repos/a", { message: "wip", lastSeeded: "" }));
    first.unmount();

    const second = renderHook(() => useCommitDrafts());
    expect(second.result.current.getCommitDraft("/repos/a")).toMatchObject({ message: "wip" });
    expect(second.result.current.getCommitDraft("/repos/b")).toBeUndefined();
  });

  it("forgets a draft once it is emptied (the commit landed)", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useCommitDrafts());
    act(() => result.current.onCommitDraftChange("/repos/a", { message: "wip", lastSeeded: "" }));
    act(() => vi.advanceTimersByTime(1000));
    act(() => result.current.onCommitDraftChange("/repos/a", { message: "", lastSeeded: "", coAuthors: [] }));
    act(() => vi.advanceTimersByTime(1000));
    expect(readCommitDraft("/repos/a")).toBeUndefined();
  });
});
