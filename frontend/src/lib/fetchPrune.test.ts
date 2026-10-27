import { afterEach, describe, expect, it, vi } from "vitest";
import type { RemoteInfo } from "../ipc/RepoClient";
import { effectiveFetchPrune, readFetchPrune, writeFetchPrune } from "./fetchPrune";

const remote = (name: string, prune?: boolean): RemoteInfo => ({
  name,
  fetchUrl: "x",
  pushUrl: null,
  authMode: null,
  authUsername: null,
  prune,
});

describe("fetch prune preference", () => {
  afterEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it("is unset until written, per repo", () => {
    expect(readFetchPrune("/a")).toBeNull();
    writeFetchPrune("/a", true);
    writeFetchPrune("/b", false);
    expect(readFetchPrune("/a")).toBe(true);
    expect(readFetchPrune("/b")).toBe(false);
    expect(localStorage.getItem("fetch-prune:/a")).toBe("on");
    expect(localStorage.getItem("fetch-prune:/b")).toBe("off");
  });

  it("survives a storage that throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(readFetchPrune("/a")).toBeNull();
    expect(() => writeFetchPrune("/a", true)).not.toThrow();
  });

  it("falls back to the fetch target remote's git config default", () => {
    const remotes = [remote("origin", false), remote("backup", true)];
    expect(effectiveFetchPrune(null, remotes, null)).toBe(false);
    expect(
      effectiveFetchPrune(null, remotes, { localBranch: "main", remoteName: "backup", remoteBranch: "main" }),
    ).toBe(true);
    expect(effectiveFetchPrune(true, remotes, null)).toBe(true);
    expect(effectiveFetchPrune(false, [remote("origin", true)], null)).toBe(false);
    expect(effectiveFetchPrune(null, [remote("origin")], null)).toBe(false);
    expect(effectiveFetchPrune(null, [], null)).toBe(false);
  });
});
