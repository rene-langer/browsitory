import { describe, expect, it, vi } from "vitest";
import {
  commitFileMenuItems,
  deleteUntrackedConfirmMessage,
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

  it("disables Stash file… with the stash rule's reason (#495)", () => {
    const stash = fileMenuItems(tracked, vi.fn(), undefined, "A rebase is in progress.").find((i) => i.label === "Stash file…")!;
    expect(stash.disabled).toBe(true);
    expect(stash.title).toBe("A rebase is in progress.");
    expect(fileMenuItems(tracked, vi.fn()).find((i) => i.label === "Stash file…")!.disabled).toBe(false);
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
    expect(discardConfirmMessage(tracked)).toBe("Discard changes to src/a.ts? Undo can bring them back.");
    expect(discardConfirmMessage(untracked)).toBe("Delete untracked file logs/run.log? Undo can bring it back.");
    expect(discardAllConfirmMessage([tracked, untracked])).toBe(
      "Discard changes to 2 files (1 untracked will be deleted)? Undo can bring them back.",
    );
    expect(discardAllConfirmMessage([tracked])).toBe("Discard changes to 1 file? Undo can bring them back.");
  });

  it("words the delete-untracked confirmation and points Discard all at it (#622)", () => {
    expect(deleteUntrackedConfirmMessage(["a.txt", "b/", "c.txt", "d.txt", "e.txt"])).toBe(
      "Delete 5 untracked files? Undo can bring them back.",
    );
    expect(deleteUntrackedConfirmMessage(["a.txt"])).toBe("Delete 1 untracked file? Undo can bring it back.");
    expect(discardAllConfirmMessage([tracked, untracked], true)).toBe(
      "Discard changes to 2 files (1 untracked will be deleted)? Undo can bring them back. To delete only " +
        'the untracked files and keep the other changes, use "Delete untracked…".',
    );
    // Nothing to point at: no untracked file, or nothing but untracked files.
    expect(discardAllConfirmMessage([tracked], true)).toBe("Discard changes to 1 file? Undo can bring them back.");
    expect(discardAllConfirmMessage([untracked], true)).toBe(
      "Discard changes to 1 file (1 untracked will be deleted)? Undo can bring them back.",
    );
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

    it("puts a commit file's revert, apply (each also staged, #536) and save first, disabling only the repository actions when blocked (#448)", () => {
      const onPick = vi.fn();
      const items = commitFileMenuItems(onPick, { tools: false, patch: { blockedReason: "A merge is in progress." } });
      expect(items.map((i) => i.label)).toEqual([
        "Revert this file",
        "Revert this file and stage",
        "Apply this file to the working tree",
        "Apply this file and stage",
        "Save file as patch…",
      ]);
      expect(items.map((i) => i.disabled ?? false)).toEqual([true, true, true, true, false]);
      expect(items[0].title).toBe("A merge is in progress.");
      for (const item of items) item.onSelect();
      expect(onPick.mock.calls.map(([action]) => action)).toEqual([
        { kind: "revertCommitFile", stage: false },
        { kind: "revertCommitFile", stage: true },
        { kind: "applyCommitFile", stage: false },
        { kind: "applyCommitFile", stage: true },
        { kind: "saveFilePatch" },
      ]);
      expect(commitFileMenuItems(onPick, { patch: { blockedReason: null } })).toHaveLength(8);
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
