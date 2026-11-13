import { GLOBAL_SHORTCUTS, type GlobalShortcut, type KeyChord } from "./keybindings";

/**
 * The VSCode extension's shortcut contributions (#679), derived from `GLOBAL_SHORTCUTS`: one
 * hidden command per shortcut VSCode binds itself (`vscodeKeybinding`), and a keybinding per key,
 * active while the Browsitory panel has the focus. `extension/package.json` holds a copy, which
 * `vscodeKeybindings.test.ts` keeps equal to this; the extension registers a command for each
 * `browsitory.shortcut.<id>` it contributes and hands the shortcut to the webview.
 */

export const SHORTCUT_COMMAND_PREFIX = "browsitory.shortcut.";

/** The panel is the active editor and its webview has the focus (a context key the extension sets). */
export const VSCODE_SHORTCUT_WHEN = "activeWebviewPanelId == 'browsitory' && browsitory.webviewFocused";

export interface VscodeCommand {
  command: string;
  title: string;
  category: string;
}

export interface VscodeKeybinding {
  command: string;
  key: string;
  mac?: string;
  when: string;
  args?: { index: number };
}

const KEY_NAMES: Record<string, string> = {
  ArrowLeft: "left",
  ArrowRight: "right",
  ArrowUp: "up",
  ArrowDown: "down",
  Tab: "tab",
  Enter: "enter",
};

/** A chord in VSCode's keybinding syntax ("ctrl+shift+t", "cmd+["). */
export function vscodeKey(chord: KeyChord, apple: boolean): string {
  const parts: string[] = [];
  if (chord.ctrl || (chord.mod && !apple)) parts.push("ctrl");
  if (chord.shift) parts.push("shift");
  if (chord.alt) parts.push("alt");
  if (chord.mod && apple) parts.push("cmd");
  parts.push(KEY_NAMES[chord.key] ?? chord.key.toLowerCase());
  return parts.join("+");
}

/** The shortcuts the extension contributes keybindings for. */
export function vscodeShortcuts(): GlobalShortcut[] {
  return GLOBAL_SHORTCUTS.filter((shortcut) => shortcut.vscodeKeybinding === true);
}

export function vscodeShortcutCommands(): VscodeCommand[] {
  return vscodeShortcuts().map((shortcut) => ({
    command: SHORTCUT_COMMAND_PREFIX + shortcut.id,
    title: shortcut.description,
    category: "Browsitory",
  }));
}

export function vscodeShortcutKeybindings(): VscodeKeybinding[] {
  return vscodeShortcuts().flatMap((shortcut) => {
    const apple = shortcut.appleChords ?? shortcut.chords;
    if (apple.length !== shortcut.chords.length) {
      throw new Error(`${shortcut.id}: a VSCode keybinding needs as many Apple chords as others`);
    }
    return shortcut.chords.map((chord, index) => {
      const key = vscodeKey(chord, false);
      const mac = vscodeKey(apple[index], true);
      return {
        command: SHORTCUT_COMMAND_PREFIX + shortcut.id,
        key,
        ...(mac === key ? {} : { mac }),
        when: VSCODE_SHORTCUT_WHEN,
        // Ctrl/Cmd+1…9: which tab.
        ...(shortcut.range === true ? { args: { index: index + 1 } } : {}),
      };
    });
  });
}

// Stub for the test-first commit (#900); the implementation commit replaces it.
export function chordFromVscodeKey(_key: string, _apple: boolean): KeyChord | null {
  return null;
}
