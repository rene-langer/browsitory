import { describe, expect, it } from "vitest";
import { comparableWorktreePath, refNameClash, takenNameMessage } from "./takenNames";

describe("refNameClash", () => {
  it("finds the same name", () => {
    expect(refNameClash(["main", "v1"], "v1")).toBe("v1");
  });

  it("finds a name that is a folder of the new one, and the other way round", () => {
    expect(refNameClash(["feature"], "feature/x")).toBe("feature");
    expect(refNameClash(["feature/x"], "feature")).toBe("feature/x");
  });

  it("compares exactly: no clash for a shared prefix, another case or a free name", () => {
    expect(refNameClash(["feature"], "feature-x")).toBeNull();
    expect(refNameClash(["feature/x"], "feature/y")).toBeNull();
    expect(refNameClash(["Feature"], "feature")).toBeNull();
    expect(refNameClash([], "anything")).toBeNull();
  });
});

describe("takenNameMessage", () => {
  it("names the taken name, or what the new one clashes with", () => {
    expect(takenNameMessage("tag", "v1", "v1")).toBe('A tag named "v1" already exists.');
    expect(takenNameMessage("branch", "feature/x", "feature")).toBe(
      '"feature/x" can\'t be used while a branch named "feature" exists.',
    );
  });
});

describe("comparableWorktreePath", () => {
  it("resolves a relative path from the base folder", () => {
    expect(comparableWorktreePath("../wt", "/work/repo")).toBe(comparableWorktreePath("/work/wt", "/elsewhere"));
    expect(comparableWorktreePath("./wt/", "/work/repo")).toBe("/work/repo/wt");
  });

  it("treats Windows separators, the \\\\?\\ prefix and case as the same folder", () => {
    const listed = comparableWorktreePath("\\\\?\\C:\\Work\\WT", "C:\\work\\repo");
    expect(comparableWorktreePath("C:\\work\\wt", "C:\\work\\repo")).toBe(listed);
    expect(comparableWorktreePath("..\\wt", "C:\\work\\repo")).toBe(listed);
    expect(comparableWorktreePath("\\\\?\\UNC\\server\\share\\wt", "C:\\")).toBe(
      comparableWorktreePath("\\\\server\\share\\wt", "C:\\"),
    );
  });

  it("keeps different folders apart", () => {
    expect(comparableWorktreePath("/work/wt", "/")).not.toBe(comparableWorktreePath("/work/wt2", "/"));
  });
});
