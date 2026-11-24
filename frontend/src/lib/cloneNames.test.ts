import { describe, expect, it } from "vitest";
import { deriveRepoName, joinPath, splitHttpsUserName } from "./cloneNames";

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

describe("splitHttpsUserName", () => {
  it.each([
    ["https://alice@bitbucket.org/ws/r.git", "https://bitbucket.org/ws/r.git", "alice"],
    ["https://ghp_abc@github.com/o/r.git", "https://github.com/o/r.git", "ghp_abc"],
    ["  HTTPS://al%40ice@host:8443/r?x=1  ", "HTTPS://host:8443/r?x=1", "al@ice"],
    ["https://alice@host", "https://host", "alice"],
  ])("%s -> %s and %s", (url, rest, username) => {
    expect(splitHttpsUserName(url)).toEqual({ url: rest, username });
  });

  it.each([
    "https://alice:secret@host/r.git",
    "https://alice:@host/r.git",
    "https://host/a@b",
    "https://host/r.git",
    "https://@host/r.git",
    "https://alice@",
    "http://alice@host/r.git",
    "ssh://alice@host/r.git",
    "alice@host:r.git",
    "",
  ])("leaves %s alone", (url) => {
    expect(splitHttpsUserName(url)).toBeNull();
  });
});
