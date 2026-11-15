import { describe, expect, it } from "vitest";
import type { StatusEntry } from "../ipc/RepoClient";
import { statusLetter } from "./statusLetter";

const entry = (staged: boolean, kind: StatusEntry["kind"]): StatusEntry => ({ path: "f.txt", oldPath: null, staged, kind });

describe("statusLetter (#1027)", () => {
  it("marks an untracked file with '?', as git status --short does, not git's 'U' for unmerged", () => {
    expect(statusLetter(entry(false, "New"))).toEqual({ letter: "?", kind: "Untracked" });
  });

  it("keeps 'A' for a staged new file and 'C' for a conflict", () => {
    expect(statusLetter(entry(true, "New"))).toEqual({ letter: "A", kind: "Added" });
    expect(statusLetter(entry(false, "Conflicted"))).toEqual({ letter: "C", kind: "Conflicted" });
  });
});
