import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NARROW_BREAKPOINT, useNarrowLayout } from "./useNarrowLayout";

describe("useNarrowLayout (#635)", () => {
  const originalWidth = window.innerWidth;
  afterEach(() => {
    vi.unstubAllGlobals();
    Object.defineProperty(window, "innerWidth", { writable: true, configurable: true, value: originalWidth });
  });

  it("follows the observed element's width, not the window's", () => {
    let callback: ResizeObserverCallback | null = null;
    const observe = vi.fn();
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(cb: ResizeObserverCallback) {
          callback = cb;
        }
        observe = observe;
        disconnect = vi.fn();
        unobserve = vi.fn();
      },
    );
    Object.defineProperty(window, "innerWidth", { writable: true, configurable: true, value: 1600 });
    const element = document.createElement("div");
    const { result } = renderHook(() => useNarrowLayout(element));
    expect(observe).toHaveBeenCalledWith(element);
    expect(result.current).toBe(false);
    const resize = (width: number) =>
      act(() => callback!([{ contentRect: { width } } as ResizeObserverEntry], {} as ResizeObserver));
    resize(NARROW_BREAKPOINT - 1);
    expect(result.current).toBe(true);
    resize(NARROW_BREAKPOINT);
    expect(result.current).toBe(false);
  });
});
