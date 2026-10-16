import { describe, expect, it, vi } from "vitest";
import { discardAllConfirmMessage, discardConfirmMessage, fileMenuItems, ignorePatterns } from "./FileActionsMenu";

const tracked = { path: "src/a.ts", staged: false, kind: "Modified" as const };
const untracked = { path: "logs/run.log", staged: false, kind: "New" as const };

describe("FileActionsMenu", () => {
  it("offers only discard for tracked files", () => {
    expect(fileMenuItems(tracked, vi.fn()).map((i) => i.label)).toEqual(["Discard changes…"]);
  });

  it("offers ignore options for untracked files", () => {
    expect(fileMenuItems(untracked, vi.fn()).map((i) => i.label)).toEqual([
      "Discard changes…", "Ignore file", "Ignore all *.log", "Ignore folder logs/",
    ]);
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
    items[2].onSelect();
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
});
