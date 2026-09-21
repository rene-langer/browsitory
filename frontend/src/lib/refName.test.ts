import { describe, expect, it } from "vitest";
import { branchNameProblem } from "./refName";

describe("branchNameProblem", () => {
  it.each(["feat/new", "fix-123", "release/1.2.3", ""])("accepts %s", (name) => {
    expect(branchNameProblem(name)).toBeNull();
  });
  it.each(["has space", "a..b", "-lead", "trail/", "x.lock", "a~b", "a:b", "end.", "a//b", "@"])("rejects %s", (name) => {
    expect(branchNameProblem(name)).not.toBeNull();
  });
});
