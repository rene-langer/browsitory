import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useTheme } from "./useTheme";

/** A `prefers-color-scheme: dark` query whose answer the test can change. */
function stubSystemScheme(dark: boolean) {
  const listeners = new Set<(event: MediaQueryListEvent) => void>();
  const query = {
    matches: dark,
    addEventListener: (_: string, listener: (event: MediaQueryListEvent) => void) => listeners.add(listener),
    removeEventListener: (_: string, listener: (event: MediaQueryListEvent) => void) => listeners.delete(listener),
  };
  vi.stubGlobal("matchMedia", () => query);
  return {
    set(next: boolean) {
      query.matches = next;
      listeners.forEach((listener) => listener({ matches: next } as MediaQueryListEvent));
    },
    listenerCount: () => listeners.size,
  };
}

beforeEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("useTheme", () => {
  it("defaults to System, leaving data-theme unset so the stylesheet follows the system", () => {
    stubSystemScheme(true);

    const { result } = renderHook(() => useTheme());

    expect(result.current.preference).toBe("system");
    expect(result.current.theme).toBe("dark");
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it("follows a system change live while on System", () => {
    const system = stubSystemScheme(false);
    const { result } = renderHook(() => useTheme());
    expect(result.current.theme).toBe("light");

    act(() => system.set(true));

    expect(result.current.theme).toBe("dark");
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it("pins an explicit choice, ignores the system, and can go back to System", () => {
    const system = stubSystemScheme(false);
    const { result } = renderHook(() => useTheme());

    act(() => result.current.setPreference("dark"));
    expect(result.current.theme).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem("browsitory-theme")).toBe("dark");

    act(() => system.set(false));
    expect(result.current.theme).toBe("dark");

    act(() => result.current.setPreference("system"));
    expect(result.current.preference).toBe("system");
    expect(result.current.theme).toBe("light");
    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(localStorage.getItem("browsitory-theme")).toBeNull();
  });

  it("starts from the stored choice", () => {
    stubSystemScheme(false);
    localStorage.setItem("browsitory-theme", "dark");

    const { result } = renderHook(() => useTheme());

    expect(result.current.preference).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("applies nothing when disabled (the VSCode webview)", () => {
    stubSystemScheme(false);
    localStorage.setItem("browsitory-theme", "dark");

    renderHook(() => useTheme(false));

    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it("stops listening when unmounted", () => {
    const system = stubSystemScheme(false);
    const { unmount } = renderHook(() => useTheme());
    expect(system.listenerCount()).toBe(1);

    unmount();

    expect(system.listenerCount()).toBe(0);
  });
});
