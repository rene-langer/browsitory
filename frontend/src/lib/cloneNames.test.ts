import { describe, expect, it } from "vitest";
import { deriveRepoName, joinPath } from "./cloneNames";

describe("deriveRepoName", () => {
  it.each([
    ["https://github.com/org/repo.git", "repo"],
    ["https://github.com/org/repo/", "repo"],
    ["git@github.com:org/repo.git", "repo"],
    ["git@host:repo.git", "repo"],
    ["ssh://git@host:22/org/repo", "repo"],
    ["/home/me/src/thing.git", "thing"],
    ["C:\\src\\thing", "thing"],
    ["  https://h/x.git  ", "x"],
    ["", ""],
  ])("%s -> %s", (url, name) => {
    expect(deriveRepoName(url)).toBe(name);
  });
});

describe("joinPath", () => {
  it("joins with the parent's separator and drops trailing separators", () => {
    expect(joinPath("/home/me/src/", "repo")).toBe("/home/me/src/repo");
    expect(joinPath("C:\\src\\", "repo")).toBe("C:\\src\\repo");
    expect(joinPath("/", "repo")).toBe("/repo");
  });
});
