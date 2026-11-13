import { useSyncExternalStore } from "react";
import {
  GLOBAL_SHORTCUTS,
  MERGE_EDITOR_SHORTCUTS,
  defaultChordsFor,
  effectiveChordsFor,
  getKeymapVersion,
  setUserChords,
  subscribeKeymap,
  type BindableId,
  type KeyChord,
} from "./keybindings";
import { isApplePlatform, isVscodeHost } from "./platform";

/**
 * The user's keys on top of the built-in keymap (#898): a shortcut gets other keys, a second
 * key, or none. `lib/keybindings.ts` reads them wherever it reads a shortcut's keys, so the
 * window shortcuts, the merge editor, the shortcut sheet, the palette and the tooltips follow.
 *
 * A per-machine preference in `localStorage`, like the other UI preferences (`lineStatsPreference.ts`).
 * Only the desktop app applies it: in VSCode the extension's keybindings run the shortcuts, and
 * VSCode's own Keyboard Shortcuts editor rebinds them (#900, owner decision).
 *
 * The reserved keys (owner decision) can't be given to any shortcut: copy, paste, cut, select
 * all, undo and redo (Ctrl/Cmd+C/V/X/A/Z/Y), Esc, Tab, Shift+Tab, Enter, Space, the arrow keys
 * without modifiers, F6 / Shift+F6, and the OS's quit and close keys (Cmd+Q, Cmd+W, Alt+F4). A
 * shortcut whose built-in keys are reserved (Undo's Ctrl+Z, F6) keeps them. Ctrl/Cmd+1 … 9 is a
 * range of keys and stays fixed.
 */

export type { BindableId };

export const KEYMAP_STORAGE_KEY = "keybindings";

export interface RebindableShortcut {
  id: BindableId;
  /** As the shortcut sheet says it. */
  description: string;
  group: "Global" | "Merge editor";
}

/** What a change did: made, or why not (another shortcut's key names it). */
export type KeymapChange =
  | { ok: true }
  | { ok: false; reason: "reserved" | "fixed" }
  | { ok: false; reason: "conflict"; conflictWith: BindableId };

type Overrides = Map<BindableId, KeyChord[]>;

let overrides: Overrides = new Map();

const REBINDABLE: readonly RebindableShortcut[] = [
  ...GLOBAL_SHORTCUTS.filter((shortcut) => shortcut.range !== true).map(
    (shortcut): RebindableShortcut => ({ id: shortcut.id, description: shortcut.description, group: "Global" }),
  ),
  ...MERGE_EDITOR_SHORTCUTS.map(
    (shortcut): RebindableShortcut => ({ id: shortcut.id, description: shortcut.description, group: "Merge editor" }),
  ),
];
const REBINDABLE_IDS: ReadonlySet<string> = new Set(REBINDABLE.map((shortcut) => shortcut.id));

/** Every id with keys, the fixed ones too: they still hold their keys against a conflict. */
const ALL_IDS: readonly BindableId[] = [
  ...GLOBAL_SHORTCUTS.map((shortcut) => shortcut.id),
  ...MERGE_EDITOR_SHORTCUTS.map((shortcut) => shortcut.id),
];

/** The shortcuts the user can give other keys, in the shortcut sheet's order. */
export function rebindableShortcuts(): RebindableShortcut[] {
  return [...REBINDABLE];
}

export function isRebindable(id: BindableId): boolean {
  return REBINDABLE_IDS.has(id);
}

/** The shortcut's keys now: the user's, else the built-in ones. */
export function effectiveChords(id: BindableId, apple: boolean = isApplePlatform()): KeyChord[] {
  return effectiveChordsFor(id, apple);
}

/** Whether the user changed the shortcut's keys. */
export function isCustomized(id: BindableId): boolean {
  return overrides.has(id);
}

function chordKey(chord: KeyChord): string {
  return chord.key.length === 1 ? chord.key.toLowerCase() : chord.key;
}

/** Whether two chords are the same keys on this platform (Ctrl is the platform modifier off Apple platforms). */
export function sameChord(a: KeyChord, b: KeyChord, apple: boolean = isApplePlatform()): boolean {
  if (chordKey(a) !== chordKey(b)) return false;
  if (Boolean(a.alt) !== Boolean(b.alt) || Boolean(a.shift) !== Boolean(b.shift)) return false;
  if (apple) return Boolean(a.mod) === Boolean(b.mod) && Boolean(a.ctrl) === Boolean(b.ctrl);
  return Boolean(a.mod || a.ctrl) === Boolean(b.mod || b.ctrl);
}

const PLAIN_RESERVED = new Set(["Escape", "Tab", "Enter", " ", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "F6"]);
const EDITING_LETTERS = new Set(["c", "v", "x", "a", "z", "y"]);

/** Whether a chord is one of the reserved keys (owner decision, #898). */
export function isReservedChord(chord: KeyChord, apple: boolean = isApplePlatform()): boolean {
  const key = chordKey(chord);
  // The platform modifier: Cmd on Apple platforms (Control is another key there), Ctrl elsewhere.
  const platformMod = apple ? Boolean(chord.mod) && !chord.ctrl : Boolean(chord.mod || chord.ctrl);
  const otherMod = apple ? Boolean(chord.ctrl) : false;
  const alt = Boolean(chord.alt);
  const shift = Boolean(chord.shift);
  const none = !platformMod && !otherMod && !alt && !shift;
  if (platformMod && !otherMod && !alt && !shift) {
    if (key.length === 1 && EDITING_LETTERS.has(key)) return true;
    if (apple && (key === "q" || key === "w")) return true;
  }
  if (none && PLAIN_RESERVED.has(chord.key)) return true;
  const shiftOnly = shift && !platformMod && !otherMod && !alt;
  if (shiftOnly && (chord.key === "Tab" || chord.key === "F6")) return true;
  const altOnly = alt && !platformMod && !otherMod && !shift;
  return altOnly && chord.key === "F4";
}

/** Reserved, unless it is one of the shortcut's own built-in keys. */
function refused(id: BindableId, chord: KeyChord, apple: boolean): boolean {
  return isReservedChord(chord, apple) && !defaultChordsFor(id, apple).some((own) => sameChord(own, chord, apple));
}

/** The other shortcut that already has `chord`, if any. Global and merge editor keys share one keymap. */
export function chordConflict(id: BindableId, chord: KeyChord, apple: boolean = isApplePlatform()): BindableId | null {
  for (const other of ALL_IDS) {
    if (other === id) continue;
    if (effectiveChordsFor(other, apple).some((theirs) => sameChord(theirs, chord, apple))) return other;
  }
  return null;
}

/** The chord with its key in canonical form and only the modifiers it has. */
function normalize(chord: KeyChord): KeyChord {
  const key = /^[A-Z]$/.test(chord.key) ? chord.key.toLowerCase() : chord.key;
  return {
    ...(chord.mod ? { mod: true } : {}),
    ...(chord.ctrl ? { ctrl: true } : {}),
    ...(chord.alt ? { alt: true } : {}),
    ...(chord.shift ? { shift: true } : {}),
    key,
  };
}

function withoutDuplicates(chords: readonly KeyChord[], apple: boolean): KeyChord[] {
  const result: KeyChord[] = [];
  for (const chord of chords.map(normalize)) {
    if (!result.some((kept) => sameChord(kept, chord, apple))) result.push(chord);
  }
  return result;
}

function sameList(a: readonly KeyChord[], b: readonly KeyChord[], apple: boolean): boolean {
  return a.length === b.length && a.every((chord, index) => sameChord(chord, b[index], apple));
}

function publish(): void {
  setUserChords(new Map(overrides));
}

function save(): void {
  try {
    if (overrides.size === 0) localStorage.removeItem(KEYMAP_STORAGE_KEY);
    else localStorage.setItem(KEYMAP_STORAGE_KEY, JSON.stringify(Object.fromEntries(overrides)));
  } catch {
    // Blocked storage: the keys still apply until the app restarts.
  }
}

/** Stores the shortcut's keys, as no override when they are its built-in ones. */
function commit(id: BindableId, chords: KeyChord[], apple: boolean): void {
  if (sameList(chords, defaultChordsFor(id, apple), apple)) overrides.delete(id);
  else overrides.set(id, chords);
  save();
  publish();
}

/** Gives the shortcut exactly these keys (an empty list removes them). */
export function setShortcutChords(id: BindableId, chords: readonly KeyChord[], apple: boolean = isApplePlatform()): KeymapChange {
  if (!isRebindable(id)) return { ok: false, reason: "fixed" };
  const next = withoutDuplicates(chords, apple);
  if (next.some((chord) => refused(id, chord, apple))) return { ok: false, reason: "reserved" };
  for (const chord of next) {
    const conflictWith = chordConflict(id, chord, apple);
    if (conflictWith !== null) return { ok: false, reason: "conflict", conflictWith };
  }
  commit(id, next, apple);
  return { ok: true };
}

/**
 * Adds a key to the shortcut. A key another shortcut has is refused, unless `replace` takes it
 * from that shortcut (which can't be a fixed one).
 */
export function addShortcutChord(
  id: BindableId,
  chord: KeyChord,
  options: { replace?: boolean } = {},
  apple: boolean = isApplePlatform(),
): KeymapChange {
  if (!isRebindable(id)) return { ok: false, reason: "fixed" };
  const added = normalize(chord);
  if (refused(id, added, apple)) return { ok: false, reason: "reserved" };
  const current = effectiveChordsFor(id, apple);
  if (current.some((own) => sameChord(own, added, apple))) return { ok: true };
  const conflictWith = chordConflict(id, added, apple);
  if (conflictWith !== null) {
    if (options.replace !== true || !isRebindable(conflictWith)) return { ok: false, reason: "conflict", conflictWith };
    commit(
      conflictWith,
      effectiveChordsFor(conflictWith, apple).filter((theirs) => !sameChord(theirs, added, apple)),
      apple,
    );
  }
  commit(id, [...current, added], apple);
  return { ok: true };
}

/** Takes a key from the shortcut; its last one leaves it without keys. */
export function removeShortcutChord(id: BindableId, chord: KeyChord, apple: boolean = isApplePlatform()): void {
  if (!isRebindable(id)) return;
  commit(
    id,
    effectiveChordsFor(id, apple).filter((own) => !sameChord(own, chord, apple)),
    apple,
  );
}

/** Back to the shortcut's built-in keys. */
export function resetShortcut(id: BindableId): void {
  if (!overrides.delete(id)) return;
  save();
  publish();
}

/** Back to the built-in keys everywhere. */
export function resetAllShortcuts(): void {
  overrides = new Map();
  save();
  publish();
}

function isChord(value: unknown): value is KeyChord {
  if (typeof value !== "object" || value === null) return false;
  const chord = value as Record<string, unknown>;
  if (typeof chord.key !== "string" || chord.key === "") return false;
  return ["mod", "ctrl", "alt", "shift"].every((flag) => chord[flag] === undefined || typeof chord[flag] === "boolean");
}

/**
 * Loads the saved keys, once at start. Drops what it can't use: unknown or fixed shortcuts,
 * malformed or reserved keys. In VSCode the built-in keys apply (#900).
 */
export function loadKeymap(apple: boolean = isApplePlatform()): void {
  overrides = new Map();
  if (!isVscodeHost()) {
    let stored: unknown;
    try {
      stored = JSON.parse(localStorage.getItem(KEYMAP_STORAGE_KEY) ?? "null");
    } catch {
      stored = null;
    }
    if (typeof stored === "object" && stored !== null && !Array.isArray(stored)) {
      for (const [id, value] of Object.entries(stored)) {
        if (!REBINDABLE_IDS.has(id) || !Array.isArray(value) || !value.every(isChord)) continue;
        const bindable = id as BindableId;
        const chords = withoutDuplicates(value, apple);
        if (chords.some((chord) => refused(bindable, chord, apple))) continue;
        overrides.set(bindable, chords);
      }
    }
  }
  publish();
}

type KeyEventLike = Pick<KeyboardEvent, "key" | "code" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">;

const MODIFIER_KEYS = new Set([
  "Shift",
  "Control",
  "Alt",
  "AltGraph",
  "Meta",
  "OS",
  "Super",
  "Hyper",
  "Fn",
  "FnLock",
  "CapsLock",
  "NumLock",
  "ScrollLock",
  "Dead",
  "Process",
  "Unidentified",
]);

/**
 * The chord a key press is, for recording a new key; `null` for a modifier on its own and for
 * the Windows/Super key, which the app can't bind. Letters and digits are the physical key when
 * the layout types something else (Option+C types "ç"), as `matchesChord` matches them;
 * punctuation drops the Shift that typing it needs.
 */
export function chordFromEvent(event: KeyEventLike, apple: boolean = isApplePlatform()): KeyChord | null {
  if (MODIFIER_KEYS.has(event.key)) return null;
  if (!apple && event.metaKey) return null;
  let key = event.key;
  const letter = /^Key([A-Z])$/.exec(event.code);
  const digit = /^Digit([0-9])$/.exec(event.code);
  if (/^[a-z0-9]$/i.test(key)) key = key.toLowerCase();
  else if (letter !== null) key = letter[1].toLowerCase();
  else if (digit !== null) key = digit[1];
  const punctuation = key.length === 1 && !/[a-z0-9]/.test(key);
  return normalize({
    key,
    mod: apple ? event.metaKey : event.ctrlKey,
    ctrl: apple && event.ctrlKey,
    alt: event.altKey,
    shift: event.shiftKey && !punctuation,
  });
}

/** Re-renders the caller whenever the user's keys change. */
export function useKeymap(): number {
  return useSyncExternalStore(subscribeKeymap, getKeymapVersion);
}
