import { describe, expect, it } from "vitest";
import {
  GLOBAL_SHORTCUTS,
  ariaKeyShortcuts,
  chordsFor,
  formatChord,
  MERGE_EDITOR_SHORTCUTS,
  matchShortcut,
  matchesChord,
  mergeEditorAriaKeyShortcuts,
  mergeEditorShortcutLabel,
  shortcutLabel,
} from "./keybindings";

const press = (key: string, mods: Partial<Record<"altKey" | "ctrlKey" | "metaKey" | "shiftKey", boolean>> & { code?: string } = {}) => ({
  key,
  code: "",
  altKey: false,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  ...mods,
});

describe("shortcut notation (#683)", () => {
  it("writes Ctrl and words outside Apple platforms, with arrows for the arrow keys", () => {
    expect(formatChord({ mod: true, shift: true, key: "z" }, false)).toBe("Ctrl+Shift+Z");
    expect(formatChord({ mod: true, alt: true, key: "b" }, false)).toBe("Ctrl+Alt+B");
    expect(formatChord({ alt: true, key: "ArrowLeft" }, false)).toBe("Alt+←");
    expect(formatChord({ key: "ArrowDown" }, false)).toBe("↓");
    expect(formatChord({ mod: true, key: "Enter" }, false)).toBe("Ctrl+Enter");
    expect(formatChord({ key: "j" }, false)).toBe("j");
  });

  it("writes Apple's symbols in Apple's order on Apple platforms", () => {
    expect(formatChord({ mod: true, shift: true, key: "z" }, true)).toBe("⇧⌘Z");
    expect(formatChord({ mod: true, alt: true, key: "b" }, true)).toBe("⌥⌘B");
    expect(formatChord({ ctrl: true, shift: true, key: "Tab" }, true)).toBe("⌃⇧⇥");
    expect(formatChord({ mod: true, key: "Enter" }, true)).toBe("⌘↩");
    expect(formatChord({ alt: true, key: "c" }, true)).toBe("⌥C");
  });

  it("labels each global shortcut for the platform", () => {
    expect(shortcutLabel("palette", false)).toBe("Ctrl+K");
    expect(shortcutLabel("palette", true)).toBe("⌘K");
    expect(shortcutLabel("redo", false)).toBe("Ctrl+Shift+Z / Ctrl+Y");
    expect(shortcutLabel("redo", true)).toBe("⇧⌘Z");
    expect(shortcutLabel("go-back", false)).toBe("Alt+←");
    expect(shortcutLabel("go-back", true)).toBe("⌘[");
    expect(shortcutLabel("tab-by-number", false)).toBe("Ctrl+1 … Ctrl+9");
    expect(shortcutLabel("tab-by-number", true)).toBe("⌘1 … ⌘9");
    expect(shortcutLabel("next-pane", false)).toBe("F6");
  });

  it("writes aria-keyshortcuts with the platform's modifier", () => {
    expect(ariaKeyShortcuts("palette", false)).toBe("Control+K");
    expect(ariaKeyShortcuts("palette", true)).toBe("Meta+K");
    expect(ariaKeyShortcuts("redo", false)).toBe("Control+Shift+Z Control+Y");
    expect(ariaKeyShortcuts("next-tab", true)).toBe("Control+Tab");
  });

  it("never uses the old Ctrl/Cmd or Up/Down notation", () => {
    for (const apple of [false, true]) {
      for (const shortcut of GLOBAL_SHORTCUTS) {
        expect(shortcutLabel(shortcut.id, apple)).not.toMatch(/Cmd|Up|Down|Left|Right/);
      }
    }
  });
});

describe("matching keys (#646)", () => {
  it("takes either Ctrl or Cmd for the platform modifier, but Ctrl+Tab only with Ctrl", () => {
    expect(matchesChord(press("k", { ctrlKey: true }), { mod: true, key: "k" })).toBe(true);
    expect(matchesChord(press("k", { metaKey: true }), { mod: true, key: "k" })).toBe(true);
    expect(matchesChord(press("Tab", { metaKey: true }), { ctrl: true, key: "Tab" })).toBe(false);
    expect(matchesChord(press("Tab", { ctrlKey: true }), { ctrl: true, key: "Tab" })).toBe(true);
  });

  it("needs exactly the chord's Shift and Alt", () => {
    expect(matchesChord(press("K", { ctrlKey: true, shiftKey: true }), { mod: true, key: "k" })).toBe(false);
    expect(matchesChord(press("k", { ctrlKey: true, altKey: true }), { mod: true, key: "k" })).toBe(false);
    expect(matchesChord(press("F5"), { key: "F5" })).toBe(true);
    expect(matchesChord(press("F5", { shiftKey: true }), { key: "F5" })).toBe(false);
  });

  it("ignores the Shift that typing punctuation needs", () => {
    expect(matchesChord(press("?", { shiftKey: true }), { key: "?" })).toBe(true);
  });

  it("follows the layout's letters, and the physical key only when the layout typed none", () => {
    // QWERTZ: the key labelled Y (code KeyZ) is Y, never Undo.
    expect(matchShortcut(press("y", { ctrlKey: true, code: "KeyZ" }), false)?.id).toBe("redo");
    // macOS Option+C types "ç".
    expect(matchShortcut(press("ç", { altKey: true, code: "KeyC" }), true)?.id).toBe("focus-commit-message");
    // AZERTY: Ctrl+& is Ctrl+1.
    expect(matchShortcut(press("&", { ctrlKey: true, code: "Digit1" }), false)?.id).toBe("tab-by-number");
  });

  it("gives every chord to one shortcut only", () => {
    for (const apple of [false, true]) {
      const seen = new Map<string, string>();
      for (const shortcut of GLOBAL_SHORTCUTS) {
        for (const chord of chordsFor(shortcut.id, apple)) {
          const name = formatChord(chord, apple);
          expect(seen.get(name), `${name} on ${apple ? "Apple" : "others"}`).toBeUndefined();
          seen.set(name, shortcut.id);
        }
      }
    }
  });

  it("finds each shortcut from its own keys", () => {
    for (const apple of [false, true]) {
      for (const shortcut of GLOBAL_SHORTCUTS) {
        for (const chord of chordsFor(shortcut.id, apple)) {
          const event = press(chord.key, {
            ctrlKey: Boolean(chord.ctrl || (chord.mod && !apple)),
            metaKey: Boolean(chord.mod && apple),
            altKey: Boolean(chord.alt),
            shiftKey: Boolean(chord.shift),
          });
          expect(matchShortcut(event, apple)?.id).toBe(shortcut.id);
        }
      }
    }
  });
});

// #764: the merge editor's own keys, bound by the pane itself, never globally.
describe("merge editor shortcuts", () => {
  it("takes a block's choices with 1 … 5 and moves between conflicts with F7 / Shift+F7", () => {
    expect(MERGE_EDITOR_SHORTCUTS.map((s) => [s.id, mergeEditorShortcutLabel(s.id, false)])).toEqual([
      ["conflict-ours", "1"],
      ["conflict-theirs", "2"],
      ["conflict-ours-then-theirs", "3"],
      ["conflict-theirs-then-ours", "4"],
      ["conflict-base", "5"],
      ["previous-conflict", "Shift+F7"],
      ["next-conflict", "F7"],
    ]);
    expect(mergeEditorShortcutLabel("previous-conflict", true)).toBe("⇧F7");
    expect(mergeEditorAriaKeyShortcuts("conflict-base")).toBe("5");
    expect(mergeEditorAriaKeyShortcuts("previous-conflict")).toBe("Shift+F7");
  });

  it("uses no key a global shortcut takes", () => {
    for (const shortcut of MERGE_EDITOR_SHORTCUTS) {
      for (const chord of shortcut.chords) {
        const event = {
          key: chord.key,
          code: `Digit${chord.key}`,
          altKey: false,
          ctrlKey: false,
          metaKey: false,
          shiftKey: Boolean(chord.shift),
        };
        expect(matchShortcut(event, false)).toBeNull();
        expect(matchShortcut(event, true)).toBeNull();
      }
    }
  });
});
