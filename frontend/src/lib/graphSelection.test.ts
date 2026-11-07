import { describe, expect, it } from "vitest";
import { coversEveryBranch, withRenamedBranch, withoutBranch } from "./graphSelection";

describe("withoutBranch", () => {
  it("needs no write when nothing is selected or the branch is not in the selection", () => {
    expect(withoutBranch(null, "a", ["b"])).toBeNull();
    expect(withoutBranch(["b"], "a", ["b"])).toBeNull();
  });
  it("drops the deleted branch", () => {
    expect(withoutBranch(["a", "b"], "a", ["b", "c"])).toEqual(["b"]);
  });
  it("deleting the only selected branch shows all branches again", () => {
    expect(withoutBranch(["a"], "a", ["b", "c"])).toEqual(["b", "c"]);
  });
});

describe("withRenamedBranch", () => {
  it("renames in place and skips unrelated selections", () => {
    expect(withRenamedBranch(["a", "b"], "a", "z")).toEqual(["z", "b"]);
    expect(withRenamedBranch(["b"], "a", "z")).toBeNull();
    expect(withRenamedBranch(null, "a", "z")).toBeNull();
  });
});

describe("coversEveryBranch", () => {
  it("is true when every branch is named, stale names or not", () => {
    expect(coversEveryBranch(["a", "b"], ["b", "a"])).toBe(true);
    expect(coversEveryBranch(["a", "b", "gone"], ["a", "b"])).toBe(true);
  });
  it("is false when a branch is missing", () => {
    expect(coversEveryBranch(["a"], ["a", "b"])).toBe(false);
  });
});
