import { describe, expect, it } from "vitest";
import {
  EMPTY_FILE_SELECTION,
  extendFileSelection,
  selectAllFiles,
  selectOnlyFile,
  selectedPaths,
  toggleFile,
} from "./fileSelection";

const paths = ["a", "b", "c", "d"];

describe("fileSelection", () => {
  it("toggles files in and out, moving the anchor to a remaining file", () => {
    const two = toggleFile(selectOnlyFile(false, "a"), false, "c");
    expect([...two.paths]).toEqual(["a", "c"]);
    expect(two.anchor).toBe("c");
    const back = toggleFile(two, false, "c");
    expect([...back.paths]).toEqual(["a"]);
    expect(back.anchor).toBe("a");
    expect(toggleFile(back, false, "a")).toEqual(EMPTY_FILE_SELECTION);
  });

  it("starts a new selection when toggling a file of the other group", () => {
    const next = toggleFile(selectOnlyFile(false, "a"), true, "b");
    expect(next).toEqual(selectOnlyFile(true, "b"));
  });

  it("extends from the anchor in list order, in either direction", () => {
    expect([...extendFileSelection(selectOnlyFile(false, "b"), false, paths, "d").paths]).toEqual(["b", "c", "d"]);
    const up = extendFileSelection(selectOnlyFile(false, "c"), false, paths, "a");
    expect([...up.paths]).toEqual(["a", "b", "c"]);
    expect(up.anchor).toBe("c");
  });

  it("degrades to a single selection without a usable anchor", () => {
    expect(extendFileSelection(EMPTY_FILE_SELECTION, false, paths, "b")).toEqual(selectOnlyFile(false, "b"));
    expect(extendFileSelection(selectOnlyFile(true, "a"), false, paths, "b")).toEqual(selectOnlyFile(false, "b"));
    expect(extendFileSelection(selectOnlyFile(false, "gone"), false, paths, "b")).toEqual(selectOnlyFile(false, "b"));
  });

  it("selects a whole group, keeping an anchor inside it", () => {
    const all = selectAllFiles(selectOnlyFile(false, "c"), false, paths);
    expect([...all.paths]).toEqual(paths);
    expect(all.anchor).toBe("c");
    expect(selectAllFiles(selectOnlyFile(true, "c"), false, paths).anchor).toBe("a");
    expect(selectAllFiles(EMPTY_FILE_SELECTION, false, [])).toEqual(EMPTY_FILE_SELECTION);
  });

  it("lists the selected paths that still exist, in list order and only for its group", () => {
    const selection = toggleFile(toggleFile(selectOnlyFile(false, "d"), false, "gone"), false, "a");
    expect(selectedPaths(selection, false, paths)).toEqual(["a", "d"]);
    expect(selectedPaths(selection, true, paths)).toEqual([]);
  });
});
