import { describe, expect, it } from "vitest";
import { branchNameProblem, remoteNameProblem, tagNameProblem } from "./refName";

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

describe("tagNameProblem", () => {
  it.each(["v1.0.0", "release/2026-11", "v1_rc@2", ""])("accepts %s", (name) => {
    expect(tagNameProblem(name)).toBeNull();
  });
  it.each(["v 1", "v1~1", "a:b", "v1..2", "x@{1}", "-x", "+x", "/v1", "v1/", "a//b", "v1.", ".v1", "a/.b", "v1.lock", "a.lock/b", "@"])(
    "rejects %s",
    (name) => {
      expect(tagNameProblem(name)).not.toBeNull();
    },
  );
});
