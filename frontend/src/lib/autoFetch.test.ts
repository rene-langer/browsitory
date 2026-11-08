import { afterEach, describe, expect, it, vi } from "vitest";
import {
  __resetAutoFetchForTests,
  autoFetchLabel,
  effectiveAutoFetchMinutes,
  firstNotice,
  lastFetchAt,
  readAutoFetchOverride,
  recordFetch,
  subscribeAutoFetchOverride,
  subscribeLastFetch,
  writeAutoFetchOverride,
} from "./autoFetch";

afterEach(() => {
  localStorage.clear();
  __resetAutoFetchForTests();
});

describe("auto-fetch override", () => {
  it("follows the app setting until a repository chooses, per repository", () => {
    expect(readAutoFetchOverride("/a")).toBeNull();
    writeAutoFetchOverride("/a", 0);
    writeAutoFetchOverride("/b", 30);
    expect(readAutoFetchOverride("/a")).toBe(0);
    expect(readAutoFetchOverride("/b")).toBe(30);
    writeAutoFetchOverride("/a", null);
    expect(readAutoFetchOverride("/a")).toBeNull();
  });

  it("ignores a stored value it can't read", () => {
    localStorage.setItem("auto-fetch:/a", "soon");
    expect(readAutoFetchOverride("/a")).toBeNull();
  });

  it("tells subscribers about a change", () => {
    const listener = vi.fn();
    const unsubscribe = subscribeAutoFetchOverride(listener);
    writeAutoFetchOverride("/a", 5);
    expect(listener).toHaveBeenCalledOnce();
    unsubscribe();
    writeAutoFetchOverride("/a", 10);
    expect(listener).toHaveBeenCalledOnce();
  });

  it("uses the override, else the app interval; 0 is off", () => {
    expect(effectiveAutoFetchMinutes(null, 10)).toBe(10);
    expect(effectiveAutoFetchMinutes(0, 10)).toBe(0);
    expect(effectiveAutoFetchMinutes(30, 0)).toBe(30);
    expect(effectiveAutoFetchMinutes(null, -1)).toBe(0);
  });

  it("labels intervals", () => {
    expect(autoFetchLabel(0)).toBe("Off");
    expect(autoFetchLabel(5)).toBe("Every 5 minutes");
    expect(autoFetchLabel(60)).toBe("Every hour");
  });
});

describe("last fetch", () => {
  it("is remembered per repository for the session", () => {
    const listener = vi.fn();
    subscribeLastFetch(listener);
    expect(lastFetchAt("/a")).toBeNull();
    recordFetch("/a", 1234);
    expect(lastFetchAt("/a")).toBe(1234);
    expect(lastFetchAt("/b")).toBeNull();
    expect(listener).toHaveBeenCalledOnce();
  });

  it("names a remote once per repository", () => {
    expect(firstNotice("/a", "origin")).toBe(true);
    expect(firstNotice("/a", "origin")).toBe(false);
    expect(firstNotice("/b", "origin")).toBe(true);
  });
});
