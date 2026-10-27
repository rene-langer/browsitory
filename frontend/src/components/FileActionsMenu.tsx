import type { StatusEntry } from "../ipc/RepoClient";
import type { ContextMenuItem } from "./primitives/ContextMenu";

export type FileAction = { kind: "discard" } | { kind: "ignore"; pattern: string } | { kind: "stash" };

export const isUntracked = (entry: StatusEntry) => !entry.staged && entry.kind === "New";

export function ignorePatterns(path: string): Array<{ label: string; pattern: string }> {
  const out = [{ label: "Ignore file", pattern: `/${path}` }];
  const name = path.split("/").pop() ?? path;
  const dot = name.lastIndexOf(".");
  if (dot > 0) {
    const ext = name.slice(dot);
    out.push({ label: `Ignore all *${ext}`, pattern: `*${ext}` });
  }
  const slash = path.lastIndexOf("/");
  if (slash > 0) {
    const folder = path.slice(0, slash + 1);
    out.push({ label: `Ignore folder ${folder}`, pattern: `/${folder}` });
  }
  return out;
}

/**
 * The per-file actions menu. Conflicted files have none; staged files only "Stash file…";
 * unstaged files also discard, and untracked ones ignore.
 */
export function fileMenuItems(entry: StatusEntry, onPick: (action: FileAction) => void): ContextMenuItem[] {
  if (entry.kind === "Conflicted") return [];
  const stash: ContextMenuItem = { label: "Stash file…", onSelect: () => onPick({ kind: "stash" }) };
  if (entry.staged) return [stash];
  const items: ContextMenuItem[] = [
    { label: "Discard changes…", destructive: true, onSelect: () => onPick({ kind: "discard" }) },
    stash,
  ];
  if (isUntracked(entry)) {
    for (const { label, pattern } of ignorePatterns(entry.path)) {
      items.push({ label, onSelect: () => onPick({ kind: "ignore", pattern }) });
    }
  }
  return items;
}

export function discardConfirmMessage(entry: StatusEntry): string {
  return isUntracked(entry)
    ? `Delete untracked file ${entry.path}? This can't be undone.`
    : `Discard changes to ${entry.path}? This can't be undone.`;
}

export function discardAllConfirmMessage(entries: StatusEntry[]): string {
  const n = entries.length;
  const m = entries.filter(isUntracked).length;
  const files = `${n} ${n === 1 ? "file" : "files"}`;
  const untracked = m > 0 ? ` (${m} untracked will be deleted)` : "";
  return `Discard changes to ${files}${untracked}? This can't be undone.`;
}
