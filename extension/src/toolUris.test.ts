import { describe, expect, it } from "vitest";
import {
  diffSides,
  fileVersionUri,
  mergeSides,
  parseFileVersionUri,
  resolveRepoFile,
} from "./toolUris";

const split = (uri: string) => {
  const url = new URL(uri);
  return { path: decodeURIComponent(url.pathname), query: url.search.slice(1) };
};

describe("toolUris", () => {
  it("round-trips a file version through its URI", () => {
    const ref = {
      repoPath: "/repos/a b",
      path: "src/x y%20z.ts",
      version: { kind: "Commit", commitId: "abc123" },
    } as const;
    const uri = fileVersionUri(ref);
    expect(uri.startsWith("browsitory-file:/src/x%20y%2520z.ts?")).toBe(true);
    expect(parseFileVersionUri(split(uri))).toEqual(ref);
  });

  it("round-trips every version kind", () => {
    for (const version of [
      { kind: "Head" },
      { kind: "Index" },
      { kind: "Stage", stage: 2 },
      { kind: "FirstParent", commitId: "ABCDEF0123" },
    ] as const) {
      const ref = { repoPath: "C:\\repos\\a", path: "a.ts", version };
      expect(parseFileVersionUri(split(fileVersionUri(ref)))).toEqual(ref);
    }
  });

  it("rejects malformed or tampered URIs", () => {
    const good = split(fileVersionUri({ repoPath: "/r", path: "a.ts", version: { kind: "Head" } }));
    const encode = (value: unknown) => Buffer.from(JSON.stringify(value), "utf8").toString("base64url");
    const bad = [
      { path: "/a.ts", query: "" },
      { path: "/a.ts", query: "%%%" },
      { path: "/a.ts", query: encode([1, 2]) },
      { path: "/a.ts", query: encode({ repoPath: "/r", path: "a.ts", version: { kind: "Nope" } }) },
      { path: "/a.ts", query: encode({ repoPath: "/r", path: "a.ts", version: { kind: "Stage", stage: 4 } }) },
      { path: "/a.ts", query: encode({ repoPath: "/r", path: "a.ts", version: { kind: "Commit", commitId: "--all" } }) },
      { path: "/a.ts", query: encode({ repoPath: "", path: "a.ts", version: { kind: "Head" } }) },
      { path: "/../x", query: encode({ repoPath: "/r", path: "../x", version: { kind: "Head" } }) },
      { path: "/a.ts", query: encode({ repoPath: "/r", path: "a.ts", version: { kind: "Head" }, extra: 1 }) },
      // The visible path must match the one actually read.
      { path: "/other.ts", query: good.query },
    ];
    for (const uri of bad) {
      expect(() => parseFileVersionUri(uri), JSON.stringify(uri)).toThrow();
    }
  });

  it("rejects paths that escape the work tree", () => {
    expect(resolveRepoFile("/repo", "src/a.ts")).toBe("/repo/src/a.ts");
    expect(resolveRepoFile("C:\\repo", "src/a.ts")).toBe("C:\\repo\\src\\a.ts");
    // A backslash is an ordinary file name character on POSIX.
    expect(resolveRepoFile("/repo", "a\\b")).toBe("/repo/a\\b");
    for (const bad of ["", "../x", "a/../../x", "a/./b", "a//b", "/etc/passwd", "C:\\x", "C:x", "\\\\srv\\share", "a\\..\\..\\x"]) {
      expect(() => resolveRepoFile("/repo", bad), bad).toThrow("Invalid path.");
      expect(() => resolveRepoFile("C:\\repo", bad), bad).toThrow("Invalid path.");
    }
    // An NTFS alternate data stream is not a file in the work tree.
    expect(() => resolveRepoFile("C:\\repo", "a.txt:stream")).toThrow("Invalid path.");
  });

  it("maps each diff source to its two sides", () => {
    const unstaged = diffSides("/r", "/r", "a.ts", { kind: "Unstaged" });
    expect(parseFileVersionUri(split(unstaged.left)).version).toEqual({ kind: "Index" });
    expect(unstaged.right).toBe("file:///r/a.ts");
    expect(unstaged.title).toBe("a.ts (Index ↔ Working Tree)");

    const staged = diffSides("/r", "/r", "a.ts", { kind: "Staged", oldPath: null });
    expect(parseFileVersionUri(split(staged.left)).version).toEqual({ kind: "Head" });
    expect(parseFileVersionUri(split(staged.right)).version).toEqual({ kind: "Index" });
    expect(staged.title).toBe("a.ts (HEAD ↔ Index)");

    const stagedRename = diffSides("/r", "/r", "b.ts", { kind: "Staged", oldPath: "a.ts" });
    expect(parseFileVersionUri(split(stagedRename.left))).toEqual({ repoPath: "/r", path: "a.ts", version: { kind: "Head" } });
    expect(parseFileVersionUri(split(stagedRename.right))).toEqual({ repoPath: "/r", path: "b.ts", version: { kind: "Index" } });

    const commit = diffSides("/r", "/r", "b.ts", { kind: "Commit", commitId: "abcdef123", oldPath: "a.ts" });
    expect(parseFileVersionUri(split(commit.left))).toEqual({
      repoPath: "/r",
      path: "a.ts",
      version: { kind: "FirstParent", commitId: "abcdef123" },
    });
    expect(commit.title).toBe("b.ts (abcdef1 ↔ parent)");

    const range = diffSides("/r", "/r", "b.ts", { kind: "Range", fromId: "1111111aa", toId: "2222222bb", oldPath: null });
    expect(parseFileVersionUri(split(range.left)).version).toEqual({ kind: "Commit", commitId: "1111111aa" });
    expect(parseFileVersionUri(split(range.right)).version).toEqual({ kind: "Commit", commitId: "2222222bb" });
    expect(range.title).toBe("b.ts (1111111 ↔ 2222222)");

    const worktree = diffSides("/r", "/r", "d/b.ts", { kind: "Range", fromId: "1111111aa", toId: null, oldPath: "a.ts" });
    expect(parseFileVersionUri(split(worktree.left))).toEqual({
      repoPath: "/r",
      path: "a.ts",
      version: { kind: "Commit", commitId: "1111111aa" },
    });
    expect(worktree.right).toBe("file:///r/d/b.ts");
    expect(worktree.title).toBe("b.ts (1111111 ↔ Working Tree)");
  });

  it("builds Windows file URIs from a Windows work tree", () => {
    const sides = diffSides("C:\\r", "C:\\r", "a b.ts", { kind: "Unstaged" });
    expect(sides.right).toBe("file:///C:/r/a%20b.ts");
  });

  it("maps merge sides to the conflict stages and the work tree file", () => {
    const sides = mergeSides("/r", "/r", "a.ts");
    expect(parseFileVersionUri(split(sides.base)).version).toEqual({ kind: "Stage", stage: 1 });
    expect(parseFileVersionUri(split(sides.ours)).version).toEqual({ kind: "Stage", stage: 2 });
    expect(parseFileVersionUri(split(sides.theirs)).version).toEqual({ kind: "Stage", stage: 3 });
    expect(sides.output).toBe("file:///r/a.ts");
  });
});
