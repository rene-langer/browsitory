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
import type { KeybindingSettings, RepoClient } from "../ipc/RepoClient";
import { logFailure } from "./logger";
import { isApplePlatform, isVscodeHost } from "./platform";

/**
 * The user's keys on top of the built-in keymap (#898): a shortcut gets other keys, a second
 * key, or none. `lib/keybindings.ts` reads them wherever it reads a shortcut's keys, so the
 * window shortcuts, the merge editor, the shortcut sheet, the palette and the tooltips follow.
 *
 * Kept in `config.toml`'s `[keybindings]` section with the other Preferences settings (owner
 * decision), through `RepoClient.getKeybindingSettings`/`setKeybindingSettings`: each changed
 * shortcut's keys as names such as "mod+shift+k" or "F4" (`chordToText`). `main.tsx` connects
 * the desktop app's client once at start (`connectKeymap`). Only the desktop app applies them:
 * in VSCode the extension's keybindings run the shortcuts, and VSCode's own Keyboard Shortcuts
 * editor rebinds them (#900, owner decision).
 *
 * The reserved keys (owner decision) can't be given to any shortcut: copy, paste, cut, select
 * all, undo and redo (Ctrl/Cmd+C/V/X/A/Z/Y), Esc, Tab, Shift+Tab, Enter, Space, the arrow keys
 * without modifiers, F6 / Shift+F6, and the OS's quit and close keys (Cmd+Q, Cmd+W, Alt+F4). A
 * shortcut whose built-in keys are reserved (Undo's Ctrl+Z, F6) keeps them. Ctrl/Cmd+1 … 9 is a
 * range of keys and stays fixed.
 */

export type { BindableId };

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

/** Where the keys are kept; `null` until `connectKeymap` (and in tests that don't connect). */
type KeymapStorage = Pick<RepoClient, "getKeybindingSettings" | "setKeybindingSettings">;

let storage: KeymapStorage | null = null;
// Set by a change: one made before the saved keys arrive wins over them.
let changed = false;

const MODIFIER_NAMES = ["mod", "ctrl", "alt", "shift"] as const;

/** A chord as `config.toml` keeps it: "mod+shift+k", "ctrl+Tab", "F4", "?" ("mod++" for plus). */
export function chordToText(chord: KeyChord): string {
  return [...MODIFIER_NAMES.filter((name) => chord[name] === true), chord.key].join("+");
}

/** The chord a kept key name is; `null` for a name it can't be (an unknown modifier, no key). */
export function chordFromText(text: string): KeyChord | null {
  const plus = text.endsWith("++") || text === "+";
  const parts = plus ? [...text.slice(0, -1).split("+").slice(0, -1), "+"] : text.split("+");
  const key = parts[parts.length - 1];
  if (key === undefined || key === "") return null;
  if (key.length > 1 && !/^[A-Z][A-Za-z0-9]*$/.test(key)) return null;
  const chord: KeyChord = { key };
  for (const name of parts.slice(0, -1)) {
    if (!(MODIFIER_NAMES as readonly string[]).includes(name)) return null;
    chord[name as (typeof MODIFIER_NAMES)[number]] = true;
  }
  return normalize(chord);
}

function save(): void {
  changed = true;
  if (storage === null) return;
  const settings: KeybindingSettings = {
    overrides: Object.fromEntries([...overrides].map(([id, chords]) => [id, chords.map(chordToText)])),
  };
  // A failed save keeps the keys for this session.
  const failed = (error: unknown) => logFailure("Saving the keyboard shortcuts failed", error);
  try {
    storage.setKeybindingSettings(settings).catch(failed);
  } catch (error) {
    failed(error);
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

/** The usable overrides among the kept ones: no unknown or fixed shortcut, no unknown or reserved key. */
function usableOverrides(kept: KeybindingSettings["overrides"], apple: boolean): Overrides {
  const usable: Overrides = new Map();
  for (const [id, keys] of Object.entries(kept)) {
    if (!REBINDABLE_IDS.has(id) || !Array.isArray(keys)) continue;
    const parsed = keys.map((key) => (typeof key === "string" ? chordFromText(key) : null));
    if (parsed.some((chord) => chord === null)) continue;
    const bindable = id as BindableId;
    const chords = withoutDuplicates(parsed as KeyChord[], apple);
    if (chords.some((chord) => refused(bindable, chord, apple))) continue;
    usable.set(bindable, chords);
  }
  return usable;
}

/**
 * Connects the keymap to where the keys are kept and loads them, once at start (`main.tsx`).
 * Drops what it can't use; an unreadable config keeps the built-in keys. In VSCode it neither
 * reads nor saves (#900).
 */
export async function connectKeymap(keptIn: KeymapStorage, apple: boolean = isApplePlatform()): Promise<void> {
  if (isVscodeHost()) return;
  storage = keptIn;
  let kept: KeybindingSettings;
  try {
    kept = await keptIn.getKeybindingSettings();
  } catch (error) {
    logFailure("Loading the keyboard shortcuts failed", error);
    return;
  }
  if (changed || storage !== keptIn) return;
  overrides = usableOverrides(kept.overrides, apple);
  publish();
}

/** Test-only: no storage, the built-in keys, nothing changed. */
export function __resetKeymapForTests(): void {
  storage = null;
  changed = false;
  overrides = new Map();
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
