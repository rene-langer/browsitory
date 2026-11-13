// Stub for the test-first commit (#898); the implementation commit replaces it.
import type { KeyChord, MergeEditorShortcutId, ShortcutId } from "./keybindings";

export type BindableId = ShortcutId | MergeEditorShortcutId;
export type KeymapChange = { ok: true } | { ok: false; reason: "reserved" | "fixed" } | { ok: false; reason: "conflict"; conflictWith: BindableId };
export interface RebindableShortcut {
  id: BindableId;
  description: string;
  group: "Global" | "Merge editor";
}

export const KEYMAP_STORAGE_KEY = "keybindings";
export const rebindableShortcuts = (): RebindableShortcut[] => [];
export const isReservedChord = (_chord: KeyChord, _apple?: boolean): boolean => false;
export const chordConflict = (_id: BindableId, _chord: KeyChord, _apple?: boolean): BindableId | null => null;
export const chordFromEvent = (_event: unknown, _apple?: boolean): KeyChord | null => null;
export const effectiveChords = (_id: BindableId, _apple?: boolean): KeyChord[] => [];
export const isCustomized = (_id: BindableId): boolean => false;
export const setShortcutChords = (_id: BindableId, _chords: KeyChord[], _apple?: boolean): KeymapChange => ({ ok: false, reason: "fixed" });
export const addShortcutChord = (_id: BindableId, _chord: KeyChord, _options?: { replace?: boolean }, _apple?: boolean): KeymapChange => ({
  ok: false,
  reason: "fixed",
});
export const removeShortcutChord = (_id: BindableId, _chord: KeyChord, _apple?: boolean): void => {};
export const resetShortcut = (_id: BindableId): void => {};
export const resetAllShortcuts = (): void => {};
export const loadKeymap = (): void => {};
