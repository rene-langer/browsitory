// The extension's manifest, which holds a copy of these contributions.
import extensionManifest from "../../../extension/package.json";
import { describe, expect, it } from "vitest";
import {
  SHORTCUT_COMMAND_PREFIX,
  chordFromVscodeKey,
  vscodeKey,
  vscodeShortcutCommands,
  vscodeShortcutKeybindings,
} from "./vscodeKeybindings";

interface PackageJson {
  contributes: {
    commands: Array<{ command: string }>;
    keybindings: Array<{ command: string }>;
    menus: { commandPalette: Array<{ command: string; when: string }> };
  };
}

const packageJson = extensionManifest as unknown as PackageJson;

const isShortcut = (entry: { command: string }) => entry.command.startsWith(SHORTCUT_COMMAND_PREFIX);

describe("the VSCode extension's shortcut keybindings (#679)", () => {
  it("writes chords in VSCode's syntax", () => {
    expect(vscodeKey({ mod: true, shift: true, key: "t" }, false)).toBe("ctrl+shift+t");
    expect(vscodeKey({ mod: true, shift: true, key: "t" }, true)).toBe("shift+cmd+t");
    expect(vscodeKey({ ctrl: true, key: "Tab" }, true)).toBe("ctrl+tab");
    expect(vscodeKey({ alt: true, key: "ArrowLeft" }, false)).toBe("alt+left");
    expect(vscodeKey({ key: "F5" }, false)).toBe("f5");
  });

  it("binds the keys VSCode claims, Ctrl/Cmd+1…9 with the tab's number", () => {
    const keybindings = vscodeShortcutKeybindings();
    expect(keybindings).toContainEqual(expect.objectContaining({ command: "browsitory.shortcut.close-tab", key: "ctrl+w", mac: "cmd+w" }));
    expect(keybindings).toContainEqual(
      expect.objectContaining({ command: "browsitory.shortcut.tab-by-number", key: "ctrl+9", args: { index: 9 } }),
    );
    expect(keybindings).toContainEqual(expect.objectContaining({ command: "browsitory.shortcut.go-back", key: "alt+left", mac: "cmd+[" }));
    // Undo keeps VSCode's own handling, which also undoes typing in a text field.
    expect(keybindings.some((binding) => binding.command.endsWith(".undo"))).toBe(false);
  });

  it("matches what extension/package.json contributes", () => {
    const { commands, keybindings, menus } = packageJson.contributes;
    // On a mismatch, copy the expected values into extension/package.json.
    expect(commands.filter(isShortcut)).toEqual(vscodeShortcutCommands());
    expect(keybindings.filter(isShortcut)).toEqual(vscodeShortcutKeybindings());
    // Keys, not commands to pick from VSCode's command palette.
    expect(menus.commandPalette.filter(isShortcut)).toEqual(
      vscodeShortcutCommands().map(({ command }) => ({ command, when: "false" })),
    );
  });
});

describe("reading VSCode's keys back (#900)", () => {
  it("turns VSCode's syntax into chords for the platform", () => {
    expect(chordFromVscodeKey("ctrl+shift+t", false)).toEqual({ mod: true, shift: true, key: "t" });
    expect(chordFromVscodeKey("shift+cmd+t", true)).toEqual({ mod: true, shift: true, key: "t" });
    expect(chordFromVscodeKey("ctrl+tab", true)).toEqual({ ctrl: true, key: "Tab" });
    expect(chordFromVscodeKey("alt+left", false)).toEqual({ alt: true, key: "ArrowLeft" });
    expect(chordFromVscodeKey("cmd+[", true)).toEqual({ mod: true, key: "[" });
    expect(chordFromVscodeKey("f5", false)).toEqual({ key: "F5" });
    expect(chordFromVscodeKey("Shift+F12", false)).toEqual({ shift: true, key: "F12" });
    expect(chordFromVscodeKey("ctrl+pagedown", false)).toEqual({ mod: true, key: "PageDown" });
    expect(chordFromVscodeKey("space", false)).toEqual({ key: " " });
  });

  it("reads back every key the extension contributes", () => {
    for (const binding of vscodeShortcutKeybindings()) {
      expect(chordFromVscodeKey(binding.key, false), binding.key).not.toBeNull();
      expect(chordFromVscodeKey(binding.mac ?? binding.key, true), binding.mac ?? binding.key).not.toBeNull();
    }
  });

  it("has no chord for a two-step key, a scan code, or a key the app can't bind", () => {
    expect(chordFromVscodeKey("ctrl+k ctrl+s", false)).toBeNull();
    expect(chordFromVscodeKey("ctrl+[BracketLeft]", false)).toBeNull();
    expect(chordFromVscodeKey("win+e", false)).toBeNull();
    expect(chordFromVscodeKey("meta+e", false)).toBeNull();
    expect(chordFromVscodeKey("", false)).toBeNull();
  });
});
