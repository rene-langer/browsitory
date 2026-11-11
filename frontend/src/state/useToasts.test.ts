import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toastDuration, useToasts } from "./useToasts";

describe("toastDuration", () => {
  it("gives a short message at least 5 seconds", () => {
    expect(toastDuration({ message: "Committed", tone: "success" })).toBe(5000);
  });

  it("grows with the message, up to 15 seconds", () => {
    const long = "x".repeat(150);
    expect(toastDuration({ message: long, tone: "info" })).toBeGreaterThan(5000);
    expect(toastDuration({ message: "x".repeat(1000), tone: "info" })).toBe(15000);
  });

  it("gives a toast with an action at least 10 seconds", () => {
    expect(toastDuration({ message: "Committed", tone: "success", action: { label: "Undo", run: () => {} } })).toBe(10000);
  });

  it("gives a toast with two actions at least 10 seconds (#733)", () => {
    const actions = [
      { label: "Push", run: () => {} },
      { label: "Undo", run: () => {} },
    ];
    expect(toastDuration({ message: "Committed", tone: "success", actions })).toBe(10000);
  });

  it("keeps a warning at least 8 seconds", () => {
    expect(toastDuration({ message: "Fetch failed", tone: "warning" })).toBe(8000);
  });

  it("keeps an error until it is dismissed", () => {
    expect(toastDuration({ message: "Push failed", tone: "error" })).toBeNull();
  });
});

describe("useToasts", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("defaults to the success tone and keeps the action", () => {
    const { result } = renderHook(() => useToasts());
    const run = vi.fn();
    act(() => result.current.push("Committed", { action: { label: "Undo", run } }));
    expect(result.current.toasts).toEqual([
      { id: 1, message: "Committed", tone: "success", action: { label: "Undo", run } },
    ]);
  });

  it("keeps two actions in order (#733)", () => {
    const { result } = renderHook(() => useToasts());
    const push = { label: "Push", run: vi.fn() };
    const undo = { label: "Undo", run: vi.fn() };
    act(() => result.current.push("Committed", { actions: [push, undo] }));
    expect(result.current.toasts).toEqual([{ id: 1, message: "Committed", tone: "success", actions: [push, undo] }]);
  });

  it("closes a toast after its duration", () => {
    const { result } = renderHook(() => useToasts());
    act(() => result.current.push("Committed"));
    act(() => vi.advanceTimersByTime(4999));
    expect(result.current.toasts).toHaveLength(1);
    act(() => vi.advanceTimersByTime(1));
    expect(result.current.toasts).toHaveLength(0);
  });

  it("keeps a toast with an action for 10 seconds", () => {
    const { result } = renderHook(() => useToasts());
    act(() => result.current.push("Committed", { action: { label: "Undo", run: () => {} } }));
    act(() => vi.advanceTimersByTime(9999));
    expect(result.current.toasts).toHaveLength(1);
    act(() => vi.advanceTimersByTime(1));
    expect(result.current.toasts).toHaveLength(0);
  });

  it("never closes an error by itself", () => {
    const { result } = renderHook(() => useToasts());
    act(() => result.current.push("Push failed", { tone: "error" }));
    act(() => vi.advanceTimersByTime(60_000));
    expect(result.current.toasts).toHaveLength(1);
  });

  it("stops the clock while paused and resumes with the time that was left", () => {
    const { result } = renderHook(() => useToasts());
    act(() => result.current.push("Committed"));
    act(() => vi.advanceTimersByTime(3000));
    act(() => result.current.setPaused(true));
    act(() => vi.advanceTimersByTime(60_000));
    expect(result.current.toasts).toHaveLength(1);
    act(() => result.current.setPaused(false));
    act(() => vi.advanceTimersByTime(1999));
    expect(result.current.toasts).toHaveLength(1);
    act(() => vi.advanceTimersByTime(1));
    expect(result.current.toasts).toHaveLength(0);
  });

  it("doesn't start the clock of a toast pushed while paused", () => {
    const { result } = renderHook(() => useToasts());
    act(() => result.current.setPaused(true));
    act(() => result.current.push("Committed"));
    act(() => vi.advanceTimersByTime(60_000));
    expect(result.current.toasts).toHaveLength(1);
    act(() => result.current.setPaused(false));
    act(() => vi.advanceTimersByTime(5000));
    expect(result.current.toasts).toHaveLength(0);
  });

  it("shows at most three toasts, dropping the oldest", () => {
    const { result } = renderHook(() => useToasts());
    act(() => {
      for (const message of ["a", "b", "c", "d"]) result.current.push(message);
    });
    expect(result.current.toasts.map((toast) => toast.message)).toEqual(["b", "c", "d"]);
  });
});
