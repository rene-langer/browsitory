import { describe, expect, it } from "vitest";
import type { StatusEntry } from "../ipc/RepoClient";
import { stageAllPaths, stageModifiedPaths } from "./stagePaths";

const entry = (path: string, kind: StatusEntry["kind"], extra: Partial<StatusEntry> = {}): StatusEntry => ({
  path,
  oldPath: null,
  staged: false,
  kind,
  ...extra,
});

describe("stage paths", () => {
  const status = [
    entry("a.txt", "Modified"),
    entry("new.txt", "New"),
    entry("conflict.txt", "Conflicted"),
    entry("secret.env", "Modified", { externalFilter: "crypt" }),
    entry("big.bin", "New", { externalFilter: "lfs" }),
    entry("staged.txt", "Modified", { staged: true }),
  ];

  it("Stage all leaves out conflicts and files under a filter Browsitory can't run (#1087)", () => {
    expect(stageAllPaths(status)).toEqual(["a.txt", "new.txt"]);
  });

  it("Stage modified leaves them out too (#1087)", () => {
    expect(stageModifiedPaths(status)).toEqual(["a.txt"]);
  });
});

describe("stage paths with a working-tree-encoding (#1156)", () => {
  const status = [
    entry("a.txt", "Modified"),
    entry("setup.ps1", "Modified", { workingTreeEncoding: "UTF-16LE-BOM" }),
    entry("new.ps1", "New", { workingTreeEncoding: "UTF-16LE-BOM" }),
  ];

  it("Stage all and Stage modified leave out files Browsitory can't convert", () => {
    expect(stageAllPaths(status)).toEqual(["a.txt"]);
    expect(stageModifiedPaths(status)).toEqual(["a.txt"]);
  });
});
