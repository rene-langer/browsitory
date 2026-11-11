import { describe, expect, it, vi } from "vitest";
import type { Command } from "./commands";
import { runShortcutCommand } from "./shortcutCommands";

const command = (overrides: Partial<Command>): Command => ({
  id: "x",
  label: "X",
  keywords: [],
  run: vi.fn(),
  group: "General",
  ...overrides,
});

describe("runShortcutCommand (#646)", () => {
  it("runs the command marked with the shortcut", () => {
    const push = command({ id: "push-branch:origin", shortcut: "push" });
    const other = command({ id: "pull" });
    expect(runShortcutCommand([other, push], "push", false)).toBeNull();
    expect(push.run).toHaveBeenCalled();
    expect(other.run).not.toHaveBeenCalled();
  });

  it("says why an unavailable command can't run", () => {
    const push = command({ shortcut: "push", disabledReason: "A merge is in progress." });
    expect(runShortcutCommand([push], "push", false)).toBe("Can't push now. A merge is in progress.");
    expect(push.run).not.toHaveBeenCalled();
  });

  it("explains a missing command, or the action in flight", () => {
    expect(runShortcutCommand([], "pull", false)).toBe("Can't pull now. No upstream set for the current branch.");
    expect(runShortcutCommand([], "stage-all", true)).toBe("Can't stage now. Another action is in progress.");
  });
});
