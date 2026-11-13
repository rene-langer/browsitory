import { isApplePlatform } from "./platform";

/**
 * The app's global keyboard shortcuts in one place (#646, #683): what each one is called, its
 * keys per platform, and how the keys are written. `useWorkspaceShortcuts` binds them, the
 * shortcut sheet (`lib/shortcuts.ts`), the command palette and the toolbar tooltips show them,
 * and the VSCode extension's contributed keybindings (#679) can be generated from them.
 *
 * The user's own keys (#898, `lib/keymap.ts`) replace a shortcut's built-in ones: every function
 * here that reads a shortcut's keys (`chordsFor`, the labels, the matching) reads the effective
 * keys. The VSCode keybindings (`lib/vscodeKeybindings.ts`) stay the built-in ones.
 *
 * Notation (`docs/CONTENT_GUIDELINES.md`): Apple platforms write the modifiers as symbols in
 * Apple's order with no separator ("⌥⌘B", "⇧⌘Z"); elsewhere they are words joined with "+"
 * ("Ctrl+Alt+B"). Arrow keys are always arrows ("Alt+←", "↑ / ↓").
 */

/** One key with its modifiers. */
export interface KeyChord {
  /**
   * `KeyboardEvent.key`: a letter or digit (matched by `code` too, since Alt changes `key` on
   * macOS), or a named key such as "F5", "Tab", "ArrowLeft", "Enter", "?" or "[".
   */
  key: string;
  /** The platform modifier: Cmd on Apple platforms, Ctrl elsewhere. Either key matches. */
  mod?: boolean;
  /** The Ctrl key itself, on every platform (Ctrl+Tab). */
  ctrl?: boolean;
  shift?: boolean;
  /** Alt, Option (⌥) on Apple platforms. */
  alt?: boolean;
}

/** The ids of the global shortcuts. Stable: the extension's keybindings refer to them (#679). */
export type ShortcutId =
  | "palette"
  | "show-shortcuts"
  | "toggle-sidebar"
  | "undo"
  | "redo"
  | "close-tab"
  | "reopen-closed-tab"
  | "go-back"
  | "go-forward"
  | "refresh"
  | "fetch"
  | "pull"
  | "push"
  | "stage-all"
  | "focus-commit-message"
  | "next-tab"
  | "previous-tab"
  | "tab-by-number"
  | "next-pane"
  | "previous-pane";

export interface GlobalShortcut {
  id: ShortcutId;
  /** For the shortcut sheet, in sentence case. */
  description: string;
  /** The keys, any of which runs it. */
  chords: KeyChord[];
  /** Apple platforms' keys, where they differ. */
  appleChords?: KeyChord[];
  /**
   * Also from a text field. Off for keys a text field uses itself (Undo, "?", Alt+letters,
   * which type characters on macOS).
   */
  inTextFields?: boolean;
  /** A host (a VSCode webview, the macOS app menu) may take these keys before the app sees them. */
  hostMayClaim?: boolean;
  /**
   * VSCode binds these keys itself, and a webview hands every key to VSCode too: the extension
   * contributes a keybinding for each, active while the Browsitory panel has the focus, which
   * runs it through the webview (#679, `lib/vscodeKeybindings.ts`). Every chord list has the
   * same length on every platform.
   */
  vscodeKeybinding?: boolean;
  /** Shown as the first and last chord ("Ctrl+1 … Ctrl+9"), not every one of them. */
  range?: boolean;
}

const DIGITS = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

/** Every global shortcut, in the order the sheet lists them. */
export const GLOBAL_SHORTCUTS: readonly GlobalShortcut[] = [
  { id: "palette", vscodeKeybinding: true, description: "Open the command palette", chords: [{ mod: true, key: "k" }], inTextFields: true },
  { id: "show-shortcuts", description: "Show keyboard shortcuts", chords: [{ key: "?" }] },
  { id: "refresh", vscodeKeybinding: true, description: "Refresh the repository", chords: [{ key: "F5" }], inTextFields: true },
  { id: "fetch", vscodeKeybinding: true, description: "Fetch", chords: [{ shift: true, key: "F5" }], inTextFields: true },
  { id: "pull", description: "Pull the current branch", chords: [{ mod: true, shift: true, key: "l" }], inTextFields: true },
  { id: "push", description: "Push the current branch", chords: [{ mod: true, shift: true, key: "k" }], inTextFields: true },
  { id: "stage-all", description: "Stage all changes", chords: [{ mod: true, shift: true, key: "a" }] },
  { id: "focus-commit-message", description: "Write the commit message (shows the working tree)", chords: [{ alt: true, key: "c" }] },
  { id: "undo", description: "Undo the last Git operation", chords: [{ mod: true, key: "z" }] },
  {
    id: "redo",
    description: "Redo",
    chords: [
      { mod: true, shift: true, key: "z" },
      { ctrl: true, key: "y" },
    ],
    appleChords: [{ mod: true, shift: true, key: "z" }],
  },
  { id: "next-pane", vscodeKeybinding: true, description: "Move to the next pane", chords: [{ key: "F6" }], inTextFields: true },
  { id: "previous-pane", vscodeKeybinding: true, description: "Move to the previous pane", chords: [{ shift: true, key: "F6" }], inTextFields: true },
  {
    id: "toggle-sidebar",
    vscodeKeybinding: true,
    description: "Show or hide the sidebar",
    chords: [{ mod: true, alt: true, key: "b" }],
    inTextFields: true,
    hostMayClaim: true,
  },
  { id: "next-tab", vscodeKeybinding: true, description: "Switch to the next tab", chords: [{ ctrl: true, key: "Tab" }], inTextFields: true, hostMayClaim: true },
  {
    id: "previous-tab",
    vscodeKeybinding: true,
    description: "Switch to the previous tab",
    chords: [{ ctrl: true, shift: true, key: "Tab" }],
    inTextFields: true,
    hostMayClaim: true,
  },
  {
    id: "tab-by-number",
    vscodeKeybinding: true,
    description: "Switch to tab 1 to 8, or the last tab",
    chords: DIGITS.map((key) => ({ mod: true, key })),
    inTextFields: true,
    hostMayClaim: true,
    range: true,
  },
  { id: "close-tab", vscodeKeybinding: true, description: "Close the active tab", chords: [{ mod: true, key: "w" }], inTextFields: true, hostMayClaim: true },
  {
    id: "reopen-closed-tab",
    vscodeKeybinding: true,
    description: "Reopen the last closed tab",
    chords: [{ mod: true, shift: true, key: "t" }],
    inTextFields: true,
    hostMayClaim: true,
  },
  {
    id: "go-back",
    vscodeKeybinding: true,
    description: "Back through the selected commits",
    chords: [{ alt: true, key: "ArrowLeft" }],
    appleChords: [{ mod: true, key: "[" }],
    hostMayClaim: true,
  },
  {
    id: "go-forward",
    vscodeKeybinding: true,
    description: "Forward through the selected commits",
    chords: [{ alt: true, key: "ArrowRight" }],
    appleChords: [{ mod: true, key: "]" }],
    hostMayClaim: true,
  },
];

const BY_ID = new Map(GLOBAL_SHORTCUTS.map((shortcut) => [shortcut.id, shortcut]));

/** A shortcut the user can give other keys (#898): a global one or a merge editor key. */
export type BindableId = ShortcutId | MergeEditorShortcutId;

// The user's keys by shortcut (#898), set by `lib/keymap.ts`; a shortcut missing here has its
// built-in keys. An empty list is a shortcut the user removed.
let userChords: ReadonlyMap<BindableId, readonly KeyChord[]> = new Map();
let keymapVersion = 0;
const keymapListeners = new Set<() => void>();

/** Replaces the user's keys; `lib/keymap.ts` owns them. */
export function setUserChords(chords: ReadonlyMap<BindableId, readonly KeyChord[]>): void {
  userChords = chords;
  keymapVersion += 1;
  for (const listener of keymapListeners) listener();
}

/** Calls `listener` whenever the user's keys change. */
export function subscribeKeymap(listener: () => void): () => void {
  keymapListeners.add(listener);
  return () => keymapListeners.delete(listener);
}

/** Changes with every change to the user's keys, for `useSyncExternalStore`. */
export function getKeymapVersion(): number {
  return keymapVersion;
}

/** The shortcut's built-in keys on this platform, whatever the user chose. */
export function defaultChordsFor(id: BindableId, apple: boolean = isApplePlatform()): KeyChord[] {
  const shortcut = BY_ID.get(id as ShortcutId);
  if (shortcut !== undefined) return (apple ? shortcut.appleChords : undefined) ?? shortcut.chords;
  return mergeEditorShortcut(id as MergeEditorShortcutId).chords;
}

/** The shortcut's keys on this platform: the user's, else the built-in ones. */
export function effectiveChordsFor(id: BindableId, apple: boolean = isApplePlatform()): KeyChord[] {
  const own = userChords.get(id);
  return own !== undefined ? [...own] : defaultChordsFor(id, apple);
}

export function globalShortcut(id: ShortcutId): GlobalShortcut {
  const shortcut = BY_ID.get(id);
  if (shortcut === undefined) throw new Error(`Unknown shortcut ${id}`);
  return shortcut;
}

/** The shortcut's keys on this platform: the user's (#898), else the built-in ones. */
export function chordsFor(id: ShortcutId, apple: boolean = isApplePlatform()): KeyChord[] {
  globalShortcut(id);
  return effectiveChordsFor(id, apple);
}

const APPLE_KEY_NAMES: Record<string, string> = {
  Enter: "↩",
  Tab: "⇥",
  Backspace: "⌫",
  Delete: "⌦",
};

const KEY_NAMES: Record<string, string> = {
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Escape: "Esc",
  " ": "Space",
  PageUp: "Page Up",
  PageDown: "Page Down",
  ContextMenu: "Menu",
};

/** A key's name as the platform writes it, without modifiers. */
export function formatKeyName(key: string, apple: boolean = isApplePlatform(), modified = false): string {
  if (apple && APPLE_KEY_NAMES[key] !== undefined) return APPLE_KEY_NAMES[key];
  if (KEY_NAMES[key] !== undefined) return KEY_NAMES[key];
  // A letter alone ("j", "s") is written as typed; with a modifier as on the keycap ("Ctrl+K").
  return key.length === 1 && modified ? key.toUpperCase() : key;
}

/** "Refresh (F5)", or the text alone for a shortcut without keys (#898). */
export function withShortcutLabel(text: string, label: string): string {
  return label === "" ? text : `${text} (${label})`;
}

/** "Ctrl+Shift+Z" or "⇧⌘Z". */
export function formatChord(chord: KeyChord, apple: boolean = isApplePlatform()): string {
  const modified = Boolean(chord.mod || chord.ctrl || chord.alt || chord.shift);
  const key = formatKeyName(chord.key, apple, modified);
  if (apple) {
    // Apple's order: Control, Option, Shift, Command.
    return `${chord.ctrl ? "⌃" : ""}${chord.alt ? "⌥" : ""}${chord.shift ? "⇧" : ""}${chord.mod ? "⌘" : ""}${key}`;
  }
  const parts: string[] = [];
  if (chord.mod || chord.ctrl) parts.push("Ctrl");
  if (chord.alt) parts.push("Alt");
  if (chord.shift) parts.push("Shift");
  parts.push(key);
  return parts.join("+");
}

/** Several alternatives, as the sheet writes them: "Ctrl+Shift+Z / Ctrl+Y". */
export function formatChords(chords: readonly KeyChord[], apple: boolean = isApplePlatform()): string {
  return chords.map((chord) => formatChord(chord, apple)).join(" / ");
}

/**
 * The shortcut's keys as the platform writes them, for the sheet, the palette and tooltips; ""
 * when the user removed them (#898).
 */
export function shortcutLabel(id: ShortcutId, apple: boolean = isApplePlatform()): string {
  const chords = chordsFor(id, apple);
  if (globalShortcut(id).range === true && chords.length > 1) {
    return `${formatChord(chords[0], apple)} … ${formatChord(chords[chords.length - 1], apple)}`;
  }
  return formatChords(chords, apple);
}

const ARIA_KEY_NAMES: Record<string, string> = { " ": "Space" };

/** The shortcut in `aria-keyshortcuts` syntax ("Control+Shift+Z Control+Y"). */
export function ariaKeyShortcuts(id: ShortcutId, apple: boolean = isApplePlatform()): string {
  return ariaChords(chordsFor(id, apple), apple);
}

/** Chords in `aria-keyshortcuts` syntax. */
export function ariaChords(chords: readonly KeyChord[], apple: boolean = isApplePlatform()): string {
  return chords
    .map((chord) => {
      const parts: string[] = [];
      if (chord.ctrl || (chord.mod && !apple)) parts.push("Control");
      if (chord.alt) parts.push("Alt");
      if (chord.shift) parts.push("Shift");
      if (chord.mod && apple) parts.push("Meta");
      parts.push(ARIA_KEY_NAMES[chord.key] ?? (chord.key.length === 1 ? chord.key.toUpperCase() : chord.key));
      return parts.join("+");
    })
    .join(" ");
}

type KeyEventLike = Pick<KeyboardEvent, "key" | "code" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">;

/**
 * Whether `event` is `chord`. Punctuation such as "?" ignores Shift, which typing it needs on
 * most layouts.
 */
export function matchesChord(event: KeyEventLike, chord: KeyChord): boolean {
  const key = chord.key;
  const single = key.length === 1;
  const alphanumeric = single && /[a-z0-9]/i.test(key);
  // The physical key only when the layout typed no plain letter or digit (Option+C types "ç",
  // a Cyrillic layout "с"); a QWERTZ Ctrl+Y stays Y, not the Z key it sits on.
  const typedPlain = /^[a-z0-9]$/i.test(event.key);
  const keyMatches = alphanumeric
    ? typedPlain
      ? event.key.toLowerCase() === key.toLowerCase()
      : event.code === (/[0-9]/.test(key) ? `Digit${key}` : `Key${key.toUpperCase()}`)
    : event.key === key;
  if (!keyMatches) return false;
  if (chord.ctrl) {
    if (!event.ctrlKey || event.metaKey) return false;
  } else if (chord.mod) {
    if (!event.ctrlKey && !event.metaKey) return false;
  } else if (event.ctrlKey || event.metaKey) {
    return false;
  }
  if (event.altKey !== Boolean(chord.alt)) return false;
  if (single && !alphanumeric) return !chord.shift || event.shiftKey;
  return event.shiftKey === Boolean(chord.shift);
}

/**
 * The merge editor's own keys (#764), bound by `ConflictResolutionPane` while its focus is inside
 * it, never globally. The digits act on the focused conflict block outside its result field (a
 * text field types them); F7 / Shift+F7 work anywhere in the editor. No global shortcut and no
 * key VSCode binds for a webview uses them.
 */
export type MergeEditorShortcutId =
  | "conflict-ours"
  | "conflict-theirs"
  | "conflict-ours-then-theirs"
  | "conflict-theirs-then-ours"
  | "conflict-base"
  | "previous-conflict"
  | "next-conflict";

export interface MergeEditorShortcut {
  id: MergeEditorShortcutId;
  /** The button's label, which the sheet and the palette use too. */
  description: string;
  chords: KeyChord[];
}

/** In the order the shortcut sheet lists them. */
export const MERGE_EDITOR_SHORTCUTS: readonly MergeEditorShortcut[] = [
  { id: "conflict-ours", description: "Accept ours", chords: [{ key: "1" }] },
  { id: "conflict-theirs", description: "Accept theirs", chords: [{ key: "2" }] },
  { id: "conflict-ours-then-theirs", description: "Ours then theirs", chords: [{ key: "3" }] },
  { id: "conflict-theirs-then-ours", description: "Theirs then ours", chords: [{ key: "4" }] },
  { id: "conflict-base", description: "Accept base", chords: [{ key: "5" }] },
  { id: "previous-conflict", description: "Previous conflict", chords: [{ shift: true, key: "F7" }] },
  { id: "next-conflict", description: "Next conflict", chords: [{ key: "F7" }] },
];

export function mergeEditorShortcut(id: MergeEditorShortcutId): MergeEditorShortcut {
  const shortcut = MERGE_EDITOR_SHORTCUTS.find((candidate) => candidate.id === id);
  if (shortcut === undefined) throw new Error(`Unknown merge editor shortcut ${id}`);
  return shortcut;
}

/** The merge editor shortcut's keys: the user's (#898), else the built-in ones. */
export function mergeEditorChordsFor(id: MergeEditorShortcutId, apple: boolean = isApplePlatform()): KeyChord[] {
  mergeEditorShortcut(id);
  return effectiveChordsFor(id, apple);
}

/** The merge editor shortcut's keys as the platform writes them ("Shift+F7", "⇧F7"). */
export function mergeEditorShortcutLabel(id: MergeEditorShortcutId, apple: boolean = isApplePlatform()): string {
  return formatChords(mergeEditorChordsFor(id, apple), apple);
}

/** The merge editor shortcut in `aria-keyshortcuts` syntax. */
export function mergeEditorAriaKeyShortcuts(id: MergeEditorShortcutId, apple: boolean = isApplePlatform()): string {
  return ariaChords(mergeEditorChordsFor(id, apple), apple);
}

/** The merge editor shortcut `event` presses, if any. */
export function matchMergeEditorShortcut(event: KeyEventLike, apple: boolean = isApplePlatform()): MergeEditorShortcut | null {
  return (
    MERGE_EDITOR_SHORTCUTS.find((shortcut) =>
      mergeEditorChordsFor(shortcut.id, apple).some((chord) => matchesChord(event, chord)),
    ) ?? null
  );
}

/** The global shortcut `event` presses on this platform, if any. */
export function matchShortcut(event: KeyEventLike, apple: boolean = isApplePlatform()): GlobalShortcut | null {
  for (const shortcut of GLOBAL_SHORTCUTS) {
    if (chordsFor(shortcut.id, apple).some((chord) => matchesChord(event, chord))) return shortcut;
  }
  return null;
}
