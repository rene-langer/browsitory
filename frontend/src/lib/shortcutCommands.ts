import type { Command } from "./commands";
import type { ShortcutId } from "./keybindings";

/** What a shortcut does, for "Can't … now." */
const VERBS: Partial<Record<ShortcutId, string>> = {
  fetch: "fetch",
  pull: "pull",
  push: "push",
  "stage-all": "stage",
};

/** Why a shortcut's command isn't in the palette at all (it builds only what can apply). */
const ABSENT_REASONS: Partial<Record<ShortcutId, string>> = {
  fetch: "No remote configured.",
  pull: "No upstream set for the current branch.",
  push: "Check out a branch to push, and add a remote.",
  "stage-all": "No changes to stage.",
};

/**
 * Runs the palette command a global shortcut stands for (#646), from `buildPaletteCommands`'
 * list, so a shortcut follows the same rules as the palette and the toolbar. Returns what to
 * tell the user when it can't run (the command's own reason, or why there is none), else `null`.
 */
export function runShortcutCommand(commands: readonly Command[], shortcut: ShortcutId, busy: boolean): string | null {
  const command = commands.find((candidate) => candidate.shortcut === shortcut);
  if (command !== undefined && command.disabledReason === undefined) {
    command.run();
    return null;
  }
  const verb = VERBS[shortcut];
  const reason =
    command?.disabledReason ?? (busy ? "Another action is in progress." : (ABSENT_REASONS[shortcut] ?? "Not available right now."));
  return verb === undefined ? reason : `Can't ${verb} now. ${reason}`;
}
