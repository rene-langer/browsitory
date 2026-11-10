import { beforeEach, describe, expect, it } from "vitest";
import { RECENT_BRANCH_LIMIT, readRecentBranches, recordRecentBranch } from "./recentBranches";

describe("recentBranches", () => {
  beforeEach(() => localStorage.clear());

  it("keeps the newest first, without duplicates, per repository and within the limit", () => {
    recordRecentBranch("/a", "one");
    recordRecentBranch("/a", "two");
    recordRecentBranch("/a", "one");
    expect(readRecentBranches("/a")).toEqual(["one", "two"]);
    expect(readRecentBranches("/b")).toEqual([]);
    for (let i = 0; i < RECENT_BRANCH_LIMIT + 2; i++) recordRecentBranch("/a", `b${i}`);
    expect(readRecentBranches("/a")).toHaveLength(RECENT_BRANCH_LIMIT);
  });

  it("reads a corrupt entry as empty", () => {
    localStorage.setItem("recent-branches:/a", "{not json");
    expect(readRecentBranches("/a")).toEqual([]);
    localStorage.setItem("recent-branches:/a", JSON.stringify(["x", 3, ""]));
    expect(readRecentBranches("/a")).toEqual(["x"]);
  });
});
