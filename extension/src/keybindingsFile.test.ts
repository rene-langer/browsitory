import * as path from "node:path";
import { describe, expect, it } from "vitest";
import { effectiveShortcutKeys, keybindingsFileCandidates, normalizeVscodeKey, parseKeybindingsJson } from "./keybindingsFile";

// The keys VSCode runs Browsitory's shortcuts with (#900): the extension's contributed keybindings
// with the user's keybindings.json on top.
const contributed = [
  { command: "browsitory.shortcut.refresh", key: "f5", when: "x" },
  { command: "browsitory.shortcut.fetch", key: "shift+f5", when: "x" },
  { command: "browsitory.shortcut.go-back", key: "alt+left", mac: "cmd+[", when: "x" },
  { command: "browsitory.shortcut.toggle-sidebar", key: "ctrl+alt+b", mac: "alt+cmd+b", when: "x" },
  { command: "browsitory.shortcut.tab-by-number", key: "ctrl+1", mac: "cmd+1", when: "x", args: { index: 1 } },
  { command: "browsitory.open", key: "ctrl+alt+g" },
];

describe("keybindings.json (#900)", () => {
  it("reads JSON with comments and trailing commas, as VSCode writes it", () => {
    const text = `// Place your key bindings in this file
[
  /* the refresh key */
  { "key": "f4", "command": "browsitory.shortcut.refresh", },
  { "key": "ctrl+k", "command": "a//b", "when": "x /* not a comment */" },
]`;
    expect(parseKeybindingsJson(text)).toEqual([
      { key: "f4", command: "browsitory.shortcut.refresh" },
      { key: "ctrl+k", command: "a//b", when: "x /* not a comment */" },
    ]);
  });

  it("reads nothing from a broken or empty file", () => {
    expect(parseKeybindingsJson("")).toEqual([]);
    expect(parseKeybindingsJson("[ { ")).toEqual([]);
    expect(parseKeybindingsJson('{ "key": "f4" }')).toEqual([]);
  });

  it("writes a key in one canonical order, in lower case", () => {
    expect(normalizeVscodeKey("Shift+Ctrl+T")).toBe("ctrl+shift+t");
    expect(normalizeVscodeKey("cmd+alt+B")).toBe("alt+cmd+b");
    expect(normalizeVscodeKey(" F5 ")).toBe("f5");
    expect(normalizeVscodeKey("ctrl+k ctrl+s")).toBe("ctrl+k ctrl+s");
  });

  it("starts from the contributed keys of the platform", () => {
    expect(effectiveShortcutKeys(contributed, [], false)).toEqual({
      refresh: ["f5"],
      fetch: ["shift+f5"],
      "go-back": ["alt+left"],
      "toggle-sidebar": ["ctrl+alt+b"],
      "tab-by-number": ["ctrl+1"],
    });
    expect(effectiveShortcutKeys(contributed, [], true)).toMatchObject({ "go-back": ["cmd+["], "toggle-sidebar": ["alt+cmd+b"] });
  });

  it("adds the user's keys and takes away the ones a minus entry removes", () => {
    const user = [
      { key: "f4", command: "browsitory.shortcut.refresh" },
      { key: "Shift+F5", command: "-browsitory.shortcut.fetch" },
      { command: "-browsitory.shortcut.go-back" },
      { key: "ctrl+shift+b", command: "browsitory.shortcut.go-back" },
    ];
    expect(effectiveShortcutKeys(contributed, user, false)).toMatchObject({
      refresh: ["f5", "f4"],
      fetch: [],
      "go-back": ["ctrl+shift+b"],
    });
  });

  it("drops a key the user gave another command everywhere, and keeps it under a when clause", () => {
    const user = [
      { key: "f5", command: "workbench.action.debug.start" },
      { key: "shift+f5", command: "workbench.action.debug.stop", when: "inDebugMode" },
    ];
    expect(effectiveShortcutKeys(contributed, user, false)).toMatchObject({ refresh: [], fetch: ["shift+f5"] });
  });

  it("ignores what isn't a keybinding", () => {
    const user = [null, 3, "f4", { key: 4, command: "browsitory.shortcut.refresh" }, { key: "f6" }];
    expect(effectiveShortcutKeys(contributed, user, false)).toMatchObject({ refresh: ["f5"] });
  });

  it("finds keybindings.json next to the global storage, in a profile or the default one", () => {
    const user = path.join("/home/u/.config/Code/User");
    expect(keybindingsFileCandidates(path.join(user, "globalStorage", "rene-langer.browsitory"))).toEqual([
      path.join(user, "keybindings.json"),
    ]);
    expect(keybindingsFileCandidates(path.join(user, "profiles", "3f2a", "globalStorage", "rene-langer.browsitory"))).toEqual([
      path.join(user, "profiles", "3f2a", "keybindings.json"),
      path.join(user, "keybindings.json"),
    ]);
  });
});
