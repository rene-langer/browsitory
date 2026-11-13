import { afterEach, describe, expect, it, vi } from "vitest";
import type { KeybindingSettings } from "../ipc/RepoClient";
import {
  chordsFor,
  matchMergeEditorShortcut,
  matchShortcut,
  mergeEditorAriaKeyShortcuts,
  mergeEditorShortcutLabel,
  shortcutLabel,
  ariaKeyShortcuts,
  type KeyChord,
} from "./keybindings";
import {
  addShortcutChord,
  applyHostKeybindings,
  chordConflict,
  chordFromEvent,
  effectiveChords,
  isCustomized,
  isReservedChord,
  connectKeymap,
  __resetKeymapForTests,
  rebindableShortcuts,
  removeShortcutChord,
  resetAllShortcuts,
  resetShortcut,
  setShortcutChords,
} from "./keymap";

const press = (
  key: string,
  mods: Partial<Record<"altKey" | "ctrlKey" | "metaKey" | "shiftKey", boolean>> & { code?: string } = {},
) => ({ key, code: "", altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, ...mods });

afterEach(() => {
  __resetKeymapForTests();
  delete document.documentElement.dataset.host;
});

// #898: the user's keys on top of the built-in keymap.
describe("keymap overrides (#898)", () => {
  it("rebinds a global shortcut: its keys, label, matching and aria-keyshortcuts follow", () => {
    expect(setShortcutChords("refresh", [{ key: "F4" }])).toEqual({ ok: true });
    expect(chordsFor("refresh", false)).toEqual([{ key: "F4" }]);
    expect(shortcutLabel("refresh", false)).toBe("F4");
    expect(ariaKeyShortcuts("refresh", false)).toBe("F4");
    expect(matchShortcut(press("F4"), false)?.id).toBe("refresh");
    expect(matchShortcut(press("F5"), false)).toBeNull();
    expect(isCustomized("refresh")).toBe(true);
    expect(isCustomized("fetch")).toBe(false);
  });

  it("adds a second chord next to the built-in one", () => {
    expect(addShortcutChord("refresh", { mod: true, key: "r" }, {}, false)).toEqual({ ok: true });
    expect(shortcutLabel("refresh", false)).toBe("F5 / Ctrl+R");
    expect(matchShortcut(press("F5"), false)?.id).toBe("refresh");
    expect(matchShortcut(press("r", { ctrlKey: true, code: "KeyR" }), false)?.id).toBe("refresh");
  });

  it("adding a chord the shortcut already has changes nothing", () => {
    expect(addShortcutChord("refresh", { key: "F5" }, {}, false)).toEqual({ ok: true });
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
    expect(isCustomized("refresh")).toBe(false);
  });

  it("removes a shortcut's keys: no label, nothing matches", () => {
    removeShortcutChord("push", { mod: true, shift: true, key: "k" }, false);
    expect(chordsFor("push", false)).toEqual([]);
    expect(shortcutLabel("push", false)).toBe("");
    expect(ariaKeyShortcuts("push", false)).toBe("");
    expect(matchShortcut(press("K", { ctrlKey: true, shiftKey: true, code: "KeyK" }), false)).toBeNull();
    expect(isCustomized("push")).toBe(true);
  });

  it("rebinds a merge editor key", () => {
    expect(setShortcutChords("conflict-ours", [{ key: "q" }])).toEqual({ ok: true });
    expect(mergeEditorShortcutLabel("conflict-ours", false)).toBe("q");
    expect(mergeEditorAriaKeyShortcuts("conflict-ours", false)).toBe("Q");
    expect(matchMergeEditorShortcut(press("q", { code: "KeyQ" }))?.id).toBe("conflict-ours");
    expect(matchMergeEditorShortcut(press("1", { code: "Digit1" }))).toBeNull();
  });

  it("resets one shortcut, or all of them", () => {
    setShortcutChords("refresh", [{ key: "F4" }]);
    setShortcutChords("fetch", [{ key: "F8" }]);
    resetShortcut("refresh");
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
    expect(chordsFor("fetch", false)).toEqual([{ key: "F8" }]);
    resetAllShortcuts();
    expect(chordsFor("fetch", false)).toEqual([{ shift: true, key: "F5" }]);
    expect(isCustomized("fetch")).toBe(false);
  });

  it("keys equal to the built-in ones are no override", () => {
    setShortcutChords("refresh", [{ key: "F4" }]);
    setShortcutChords("refresh", [{ key: "F5" }]);
    expect(isCustomized("refresh")).toBe(false);
  });

  it("keeps Ctrl/Cmd+1 … 9 fixed: a range is not rebindable", () => {
    expect(rebindableShortcuts().map((shortcut) => shortcut.id)).not.toContain("tab-by-number");
    expect(setShortcutChords("tab-by-number", [{ key: "F9" }])).toEqual({ ok: false, reason: "fixed" });
    expect(chordsFor("tab-by-number", false)).toHaveLength(9);
  });

  it("lists every other global shortcut and the merge editor keys as rebindable", () => {
    const ids = rebindableShortcuts().map((shortcut) => shortcut.id);
    expect(ids).toContain("refresh");
    expect(ids).toContain("palette");
    expect(ids).toContain("conflict-ours");
    expect(ids).toContain("next-conflict");
    expect(rebindableShortcuts().find((shortcut) => shortcut.id === "conflict-ours")).toMatchObject({
      description: "Accept ours",
      group: "Merge editor",
    });
    expect(rebindableShortcuts().find((shortcut) => shortcut.id === "refresh")).toMatchObject({
      description: "Refresh the repository",
      group: "Global",
    });
  });
});

describe("conflicts (#898)", () => {
  it("names the shortcut that already has the chord", () => {
    expect(chordConflict("refresh", { mod: true, shift: true, key: "k" }, false)).toBe("push");
    expect(chordConflict("refresh", { mod: true, shift: true, key: "K" }, false)).toBe("push");
    // The merge editor's keys and the global ones share one keymap.
    expect(chordConflict("refresh", { key: "1" }, false)).toBe("conflict-ours");
    expect(chordConflict("conflict-ours", { key: "F5" }, false)).toBe("refresh");
  });

  it("is no conflict with the shortcut's own keys, nor with a removed one", () => {
    expect(chordConflict("push", { mod: true, shift: true, key: "k" }, false)).toBeNull();
    removeShortcutChord("push", { mod: true, shift: true, key: "k" }, false);
    expect(chordConflict("refresh", { mod: true, shift: true, key: "k" }, false)).toBeNull();
  });

  it("compares the keys as the platform presses them", () => {
    // Ctrl is the platform modifier outside Apple platforms…
    expect(chordConflict("refresh", { ctrl: true, key: "k" }, false)).toBe("palette");
    // …but Control and Command are different keys on a Mac.
    expect(chordConflict("refresh", { ctrl: true, key: "k" }, true)).toBeNull();
    expect(chordConflict("refresh", { mod: true, key: "k" }, true)).toBe("palette");
  });

  it("refuses a chord in use, and Replace takes it from the other shortcut", () => {
    const chord: KeyChord = { mod: true, shift: true, key: "k" };
    expect(addShortcutChord("refresh", chord, {}, false)).toEqual({ ok: false, reason: "conflict", conflictWith: "push" });
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
    expect(addShortcutChord("refresh", chord, { replace: true }, false)).toEqual({ ok: true });
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }, chord]);
    expect(chordsFor("push", false)).toEqual([]);
    expect(matchShortcut(press("K", { ctrlKey: true, shiftKey: true, code: "KeyK" }), false)?.id).toBe("refresh");
  });

  it("refuses a whole list with a chord in use", () => {
    expect(setShortcutChords("refresh", [{ key: "F4" }, { shift: true, key: "F5" }], false)).toEqual({
      ok: false,
      reason: "conflict",
      conflictWith: "fetch",
    });
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
  });
});

describe("reserved keys (#898, owner decision)", () => {
  const reservedEverywhere: KeyChord[] = [
    { mod: true, key: "c" },
    { mod: true, key: "v" },
    { mod: true, key: "x" },
    { mod: true, key: "a" },
    { mod: true, key: "z" },
    { mod: true, key: "y" },
    { key: "Escape" },
    { key: "Tab" },
    { shift: true, key: "Tab" },
    { key: "Enter" },
    { key: " " },
    { key: "ArrowUp" },
    { key: "ArrowDown" },
    { key: "ArrowLeft" },
    { key: "ArrowRight" },
    { key: "F6" },
    { shift: true, key: "F6" },
  ];

  it.each(reservedEverywhere)("reserves %o on every platform", (chord) => {
    expect(isReservedChord(chord, false)).toBe(true);
    expect(isReservedChord(chord, true)).toBe(true);
  });

  it("reserves Ctrl+C written with the Ctrl key outside Apple platforms, and Alt+F4", () => {
    expect(isReservedChord({ ctrl: true, key: "c" }, false)).toBe(true);
    expect(isReservedChord({ alt: true, key: "F4" }, false)).toBe(true);
  });

  it("reserves Cmd+Q and Cmd+W on Apple platforms only", () => {
    expect(isReservedChord({ mod: true, key: "q" }, true)).toBe(true);
    expect(isReservedChord({ mod: true, key: "w" }, true)).toBe(true);
    expect(isReservedChord({ mod: true, key: "q" }, false)).toBe(false);
    expect(isReservedChord({ mod: true, key: "w" }, false)).toBe(false);
    // Control+C is no copy on a Mac.
    expect(isReservedChord({ ctrl: true, key: "c" }, true)).toBe(false);
  });

  it("leaves the keys around them free", () => {
    for (const chord of [
      { mod: true, shift: true, key: "c" },
      { mod: true, key: "Enter" },
      { alt: true, key: "ArrowLeft" },
      { shift: true, key: "ArrowUp" },
      { key: "F7" },
      { key: "F4" },
      { mod: true, alt: true, key: "z" },
    ] satisfies KeyChord[]) {
      expect(isReservedChord(chord, false)).toBe(false);
    }
  });

  it("refuses a reserved chord", () => {
    expect(addShortcutChord("refresh", { key: "Escape" }, {}, false)).toEqual({ ok: false, reason: "reserved" });
    expect(addShortcutChord("palette", { mod: true, key: "y" }, { replace: true }, false)).toEqual({
      ok: false,
      reason: "reserved",
    });
    expect(setShortcutChords("refresh", [{ key: "F6" }], false)).toEqual({ ok: false, reason: "reserved" });
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
  });

  it("lets a shortcut keep its own built-in reserved keys", () => {
    removeShortcutChord("redo", { mod: true, shift: true, key: "z" }, false);
    expect(chordsFor("redo", false)).toEqual([{ ctrl: true, key: "y" }]);
    expect(addShortcutChord("redo", { mod: true, shift: true, key: "z" }, {}, false)).toEqual({ ok: true });
    expect(setShortcutChords("next-pane", [{ key: "F6" }, { key: "F8" }], false)).toEqual({ ok: true });
    expect(chordsFor("next-pane", false)).toEqual([{ key: "F6" }, { key: "F8" }]);
  });
});

describe("recording a chord from a key press (#898)", () => {
  it("writes letters in lower case with their modifiers", () => {
    expect(chordFromEvent(press("K", { ctrlKey: true, shiftKey: true, code: "KeyK" }), false)).toEqual({
      mod: true,
      shift: true,
      key: "k",
    });
    expect(chordFromEvent(press("r", { code: "KeyR" }), false)).toEqual({ key: "r" });
    expect(chordFromEvent(press("F4", { code: "F4" }), false)).toEqual({ key: "F4" });
  });

  it("takes the physical letter when Option types another character", () => {
    expect(chordFromEvent(press("ç", { altKey: true, code: "KeyC" }), true)).toEqual({ alt: true, key: "c" });
  });

  it("drops the Shift that typing punctuation needs", () => {
    expect(chordFromEvent(press("?", { shiftKey: true, code: "Slash" }), false)).toEqual({ key: "?" });
  });

  it("maps Command and Control on Apple platforms", () => {
    expect(chordFromEvent(press("k", { metaKey: true, code: "KeyK" }), true)).toEqual({ mod: true, key: "k" });
    expect(chordFromEvent(press("k", { ctrlKey: true, code: "KeyK" }), true)).toEqual({ ctrl: true, key: "k" });
  });

  it("ignores a modifier on its own and the Windows/Super key", () => {
    expect(chordFromEvent(press("Shift", { shiftKey: true, code: "ShiftLeft" }), false)).toBeNull();
    expect(chordFromEvent(press("Control", { ctrlKey: true, code: "ControlLeft" }), false)).toBeNull();
    expect(chordFromEvent(press("Meta", { metaKey: true, code: "MetaLeft" }), true)).toBeNull();
    expect(chordFromEvent(press("k", { metaKey: true, code: "KeyK" }), false)).toBeNull();
  });
});

// Owner decision on #898: the keys are kept in config.toml (`[keybindings]`), through the client.
describe("persistence in config.toml (#898)", () => {
  function fakeStorage(stored: Record<string, string[]> = {}) {
    const saved: KeybindingSettings[] = [];
    const storage = {
      getKeybindingSettings: vi.fn(async (): Promise<KeybindingSettings> => ({ overrides: stored })),
      setKeybindingSettings: vi.fn(async (settings: KeybindingSettings) => {
        saved.push(settings);
      }),
    };
    return { storage, saved };
  }

  it("saves the overrides as key names and loads them back", async () => {
    const first = fakeStorage();
    await connectKeymap(first.storage, false);
    setShortcutChords("refresh", [{ key: "F4" }], false);
    removeShortcutChord("push", { mod: true, shift: true, key: "k" }, false);
    setShortcutChords("pull", [{ mod: true, alt: true, key: "p" }], false);
    setShortcutChords("go-back", [{ ctrl: true, shift: true, key: "ArrowLeft" }], false);
    setShortcutChords("show-shortcuts", [{ key: "?" }, { key: "/" }], false);
    expect(first.saved.at(-1)).toEqual({
      overrides: {
        refresh: ["F4"],
        push: [],
        pull: ["mod+alt+p"],
        "go-back": ["ctrl+shift+ArrowLeft"],
        "show-shortcuts": ["?", "/"],
      },
    });

    // A new start: the module forgets, then loads what was saved.
    __resetKeymapForTests();
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
    await connectKeymap(fakeStorage(first.saved.at(-1)!.overrides).storage, false);
    expect(chordsFor("refresh", false)).toEqual([{ key: "F4" }]);
    expect(chordsFor("push", false)).toEqual([]);
    expect(chordsFor("pull", false)).toEqual([{ mod: true, alt: true, key: "p" }]);
    expect(chordsFor("go-back", false)).toEqual([{ ctrl: true, shift: true, key: "ArrowLeft" }]);
    expect(chordsFor("show-shortcuts", false)).toEqual([{ key: "?" }, { key: "/" }]);
    expect(effectiveChords("refresh", false)).toEqual([{ key: "F4" }]);
  });

  it("saves no override for keys back at the built-in ones", async () => {
    const { storage, saved } = fakeStorage();
    await connectKeymap(storage, false);
    setShortcutChords("refresh", [{ key: "F4" }], false);
    resetAllShortcuts();
    expect(saved.at(-1)).toEqual({ overrides: {} });
  });

  it("drops what it can't use: unknown ids, unknown or reserved keys, fixed shortcuts", async () => {
    await connectKeymap(
      fakeStorage({
        refresh: ["F4"],
        bogus: ["F9"],
        push: ["Escape"],
        pull: ["mod+"],
        fetch: ["hyper+F8"],
        "tab-by-number": ["F9"],
      }).storage,
      false,
    );
    expect(chordsFor("refresh", false)).toEqual([{ key: "F4" }]);
    expect(chordsFor("push", false)).toEqual([{ mod: true, shift: true, key: "k" }]);
    expect(chordsFor("pull", false)).toEqual([{ mod: true, shift: true, key: "l" }]);
    expect(chordsFor("fetch", false)).toEqual([{ shift: true, key: "F5" }]);
    expect(chordsFor("tab-by-number", false)).toHaveLength(9);
  });

  it("starts with the built-in keys when the settings can't be read", async () => {
    const { storage } = fakeStorage();
    storage.getKeybindingSettings.mockRejectedValue(new Error("broken config"));
    await expect(connectKeymap(storage, false)).resolves.toBeUndefined();
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
  });

  it("keeps a change for the session when saving fails", async () => {
    const { storage } = fakeStorage();
    storage.setKeybindingSettings.mockRejectedValue(new Error("read-only"));
    await connectKeymap(storage, false);
    expect(setShortcutChords("refresh", [{ key: "F4" }], false)).toEqual({ ok: true });
    await Promise.resolve();
    expect(chordsFor("refresh", false)).toEqual([{ key: "F4" }]);
  });

  it("keeps a change made before the saved keys arrive", async () => {
    let answer: (settings: KeybindingSettings) => void = () => {};
    const { storage, saved } = fakeStorage();
    storage.getKeybindingSettings.mockReturnValue(new Promise((resolve) => (answer = resolve)));
    const connected = connectKeymap(storage, false);
    setShortcutChords("refresh", [{ key: "F4" }], false);
    answer({ overrides: { refresh: ["F8"], fetch: ["F9"] } });
    await connected;
    expect(chordsFor("refresh", false)).toEqual([{ key: "F4" }]);
    expect(saved.at(-1)).toEqual({ overrides: { refresh: ["F4"] } });
  });

  it("neither reads nor saves in VSCode, whose own Keyboard Shortcuts rebind there (#900)", async () => {
    document.documentElement.dataset.host = "vscode";
    const { storage } = fakeStorage({ refresh: ["F4"] });
    await connectKeymap(storage, false);
    expect(storage.getKeybindingSettings).not.toHaveBeenCalled();
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
  });
});

describe("VSCode's keys (#900)", () => {
  it("shows and matches the keys VSCode runs the shortcuts with", async () => {
    document.documentElement.dataset.host = "vscode";
    const setKeybindingSettings = vi.fn(async () => {});
    await connectKeymap({ getKeybindingSettings: async () => ({ overrides: {} }), setKeybindingSettings }, false);
    applyHostKeybindings(
      { refresh: ["f4"], "toggle-sidebar": ["ctrl+alt+b", "ctrl+shift+e"], "go-back": [], "close-tab": ["ctrl+k ctrl+w"] },
      false,
    );
    expect(chordsFor("refresh", false)).toEqual([{ key: "F4" }]);
    expect(shortcutLabel("toggle-sidebar", false)).toBe("Ctrl+Alt+B / Ctrl+Shift+E");
    expect(chordsFor("go-back", false)).toEqual([]);
    expect(matchShortcut(press("F4"), false)?.id).toBe("refresh");
    // A two-step key can't be shown or matched here; VSCode still runs it.
    expect(chordsFor("close-tab", false)).toEqual([]);
    // They are VSCode's keys, not the user's overrides here.
    expect(isCustomized("refresh")).toBe(false);
    expect(setKeybindingSettings).not.toHaveBeenCalled();
  });

  it("leaves the shortcuts the webview binds itself, and the range, to their built-in keys", () => {
    document.documentElement.dataset.host = "vscode";
    applyHostKeybindings({ pull: ["f9"], "tab-by-number": ["ctrl+2"], "conflict-ours": ["q"], bogus: ["f8"] }, false);
    expect(chordsFor("pull", false)).toEqual([{ mod: true, shift: true, key: "l" }]);
    expect(chordsFor("tab-by-number", false)).toHaveLength(9);
    expect(shortcutLabel("palette", false)).toBe("Ctrl+K");
  });

  it("goes back to the built-in keys for a shortcut VSCode no longer lists", () => {
    document.documentElement.dataset.host = "vscode";
    applyHostKeybindings({ refresh: ["f4"] }, false);
    applyHostKeybindings({}, false);
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
  });
});
