import { describe, expect, it } from "vitest";
import { branchNameProblem, remoteNameProblem } from "./refName";

describe("branchNameProblem", () => {
  it.each(["feat/new", "fix-123", "release/1.2.3", ""])("accepts %s", (name) => {
    expect(branchNameProblem(name)).toBeNull();
  });
  it.each(["has space", "a..b", "-lead", "trail/", "x.lock", "a~b", "a:b", "end.", "a//b", "@"])("rejects %s", (name) => {
    expect(branchNameProblem(name)).not.toBeNull();
  });
});

describe("remoteNameProblem", () => {
  it.each(["origin", "my-fork", "team/fork", "fork.v2", ""])("accepts %s", (name) => {
    expect(remoteNameProblem(name)).toBeNull();
  });
  it.each(["has space", "a..b", "trail/", "/lead", "x.lock", "a/x.lock/b", ".hidden", "a/.b", "a~b", "a:b", "a//b", "a@{b"])(
    "rejects %s",
    (name) => {
      expect(remoteNameProblem(name)).not.toBeNull();
    },
  );
});
