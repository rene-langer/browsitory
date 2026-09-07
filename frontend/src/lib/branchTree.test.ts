import { describe, expect, it } from "vitest";
import { buildBranchTree } from "./branchTree";

describe("buildBranchTree", () => {
  it("returns a flat list of leaves, sorted by name, when no path contains a slash", () => {
    const tree = buildBranchTree([
      { path: "main", value: "main-value" },
      { path: "develop", value: "develop-value" },
    ]);
    expect(tree).toEqual([
      { kind: "leaf", name: "develop", path: "develop", value: "develop-value" },
      { kind: "leaf", name: "main", path: "main", value: "main-value" },
    ]);
  });

  it("splits a path on each / into nested folders, with the value on the final segment", () => {
    const tree = buildBranchTree([{ path: "feat/foo", value: "foo-value" }]);
    expect(tree).toEqual([
      {
        kind: "folder",
        name: "feat",
        path: "feat",
        children: [{ kind: "leaf", name: "foo", path: "feat/foo", value: "foo-value" }],
      },
    ]);
  });

  it("groups multiple branches under a shared folder prefix", () => {
    const tree = buildBranchTree([
      { path: "feat/foo", value: "foo-value" },
      { path: "feat/bar", value: "bar-value" },
    ]);
    expect(tree).toEqual([
      {
        kind: "folder",
        name: "feat",
        path: "feat",
        children: [
          { kind: "leaf", name: "bar", path: "feat/bar", value: "bar-value" },
          { kind: "leaf", name: "foo", path: "feat/foo", value: "foo-value" },
        ],
      },
    ]);
  });

  it("nests folders arbitrarily deep", () => {
    const tree = buildBranchTree([{ path: "a/b/c", value: "c-value" }]);
    expect(tree).toEqual([
      {
        kind: "folder",
        name: "a",
        path: "a",
        children: [
          {
            kind: "folder",
            name: "b",
            path: "a/b",
            children: [{ kind: "leaf", name: "c", path: "a/b/c", value: "c-value" }],
          },
        ],
      },
    ]);
  });

  it("sorts folders before leaves at the same level, each alphabetically", () => {
    const tree = buildBranchTree([
      { path: "zzz", value: "zzz-value" },
      { path: "aaa/nested", value: "nested-value" },
      { path: "bbb", value: "bbb-value" },
    ]);
    expect(tree.map((node) => node.name)).toEqual(["aaa", "bbb", "zzz"]);
    expect(tree[0].kind).toBe("folder");
    expect(tree[1].kind).toBe("leaf");
    expect(tree[2].kind).toBe("leaf");
  });
});
