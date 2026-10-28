import { describe, expect, it, vi } from "vitest";
import {
  commitFileMenuItems,
  discardAllConfirmMessage,
  discardConfirmMessage,
  fileMenuItems,
  ignorePatterns,
} from "./FileActionsMenu";

const tracked = { path: "src/a.ts", oldPath: null, staged: false, kind: "Modified" as const };
const untracked = { path: "logs/run.log", oldPath: null, staged: false, kind: "New" as const };

describe("FileActionsMenu", () => {
  it("offers discard and stash for tracked files", () => {
    expect(fileMenuItems(tracked, vi.fn()).map((i) => i.label)).toEqual(["Discard changes…", "Stash file…"]);
  });

  it("offers ignore options for untracked files", () => {
    expect(fileMenuItems(untracked, vi.fn()).map((i) => i.label)).toEqual([
      "Discard changes…", "Stash file…", "Ignore file", "Ignore all *.log", "Ignore folder logs/",
    ]);
  });

  it("offers only stash for staged files and nothing for conflicted ones", () => {
    const onPick = vi.fn();
    const items = fileMenuItems({ path: "src/a.ts", oldPath: null, staged: true, kind: "Modified" }, onPick);
    expect(items.map((i) => i.label)).toEqual(["Stash file…"]);
    items[0].onSelect();
    expect(onPick).toHaveBeenCalledWith({ kind: "stash" });
    expect(fileMenuItems({ path: "c.txt", oldPath: null, staged: false, kind: "Conflicted" }, vi.fn())).toEqual([]);
  });

  it("builds root-anchored ignore patterns", () => {
    expect(ignorePatterns("logs/run.log")).toEqual([
      { label: "Ignore file", pattern: "/logs/run.log" },
      { label: "Ignore all *.log", pattern: "*.log" },
      { label: "Ignore folder logs/", pattern: "/logs/" },
    ]);
    expect(ignorePatterns("Makefile")).toEqual([{ label: "Ignore file", pattern: "/Makefile" }]);
  });

  it("marks discard destructive and reports the picked action", () => {
    const onPick = vi.fn();
    const items = fileMenuItems(untracked, onPick);
    expect(items[0].destructive).toBe(true);
    items[3].onSelect();
    expect(onPick).toHaveBeenCalledWith({ kind: "ignore", pattern: "*.log" });
  });

  it("words the confirm dialogs", () => {
    expect(discardConfirmMessage(tracked)).toBe("Discard changes to src/a.ts? This can't be undone.");
    expect(discardConfirmMessage(untracked)).toBe("Delete untracked file logs/run.log? This can't be undone.");
    expect(discardAllConfirmMessage([tracked, untracked])).toBe(
      "Discard changes to 2 files (1 untracked will be deleted)? This can't be undone.",
    );
    expect(discardAllConfirmMessage([tracked])).toBe("Discard changes to 1 file? This can't be undone.");
  });

  describe("external tools", () => {
    const conflicted = { path: "c.txt", oldPath: null, staged: false, kind: "Conflicted" as const };
    const deleted = { path: "gone.txt", oldPath: null, staged: false, kind: "Deleted" as const };
    const idle = { mergeToolRunning: false };

    it("adds external tool items only when tools are available", () => {
      expect(fileMenuItems(tracked, vi.fn(), idle).map((i) => i.label)).toEqual([
        "Discard changes…",
        "Stash file…",
        "Open in editor",
        "Reveal in file manager",
        "Open in diff tool",
      ]);
      expect(fileMenuItems({ ...tracked, staged: true }, vi.fn(), idle).map((i) => i.label)).toEqual([
        "Stash file…",
        "Open in editor",
        "Reveal in file manager",
        "Open in diff tool",
      ]);
    });

    it("offers the merge tool for conflicted files and disables it while running", () => {
      const onPick = vi.fn();
      const items = fileMenuItems(conflicted, onPick, { mergeToolRunning: true });
      expect(items.map((i) => i.label)).toEqual(["Open in editor", "Reveal in file manager", "Open in merge tool"]);
      expect(items[2].disabled).toBe(true);
      fileMenuItems(conflicted, onPick, idle)[2].onSelect();
      expect(onPick).toHaveBeenCalledWith({ kind: "mergeTool" });
    });

    it("disables open in editor for a deleted file", () => {
      const open = fileMenuItems(deleted, vi.fn(), idle).find((i) => i.label === "Open in editor");
      expect(open?.disabled).toBe(true);
      expect(open?.title).toBe("The file was deleted");
    });

    it("reports external tool picks", () => {
      const onPick = vi.fn();
      const items = fileMenuItems(tracked, onPick, idle);
      for (const item of items.slice(2)) item.onSelect();
      expect(onPick.mock.calls.map(([action]) => action)).toEqual([
        { kind: "openInEditor" },
        { kind: "reveal" },
        { kind: "diffTool" },
      ]);
    });

    it("builds the commit file menu and reports picks", () => {
      const onPick = vi.fn();
      const items = commitFileMenuItems(onPick);
      expect(items.map((i) => i.label)).toEqual(["Open in editor", "Reveal in file manager", "Open in diff tool"]);
      items[2].onSelect();
      expect(onPick).toHaveBeenCalledWith({ kind: "diffTool" });
    });
  });
});
