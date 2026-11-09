import type { StatusEntry } from "../ipc/RepoClient";
import type { ContextMenuItem } from "./primitives/ContextMenu";

export type FileAction =
  | { kind: "discard" }
  | { kind: "ignore"; pattern: string }
  | { kind: "stash" }
  | { kind: "openInEditor" }
  | { kind: "reveal" }
  | { kind: "diffTool" }
  | { kind: "mergeTool" }
  // A multi-file selection's actions (#449): they act on every selected file, not just this one.
  | { kind: "stageSelection" }
  | { kind: "unstageSelection" }
  | { kind: "discardSelection" }
  | { kind: "ignoreSelection" }
  | { kind: "stashSelection" };

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
 * The per-file actions menu. Conflicted files have no repo actions; staged files only
 * "Stash file…"; unstaged files also discard, and untracked ones ignore. With `tools` (external
 * tools are available), every entry also gets editor/file manager items plus its diff tool, or
 * for a conflicted file its merge tool (disabled while that tool is open).
 */
export function fileMenuItems(
  entry: StatusEntry,
  onPick: (action: FileAction) => void,
  tools?: { mergeToolRunning: boolean },
  /** Why stashing is unavailable (`stashBlockReason`), disabling "Stash file…". */
  stashDisabledReason: string | null = null,
): ContextMenuItem[] {
  const items: ContextMenuItem[] = [];
  if (entry.kind !== "Conflicted") {
    const stash: ContextMenuItem = {
      label: "Stash file…",
      onSelect: () => onPick({ kind: "stash" }),
      disabled: stashDisabledReason !== null,
      title: stashDisabledReason ?? undefined,
    };
    if (entry.staged) {
      items.push(stash);
    } else {
      items.push({ label: "Discard changes…", destructive: true, onSelect: () => onPick({ kind: "discard" }) }, stash);
      if (isUntracked(entry)) {
        for (const { label, pattern } of ignorePatterns(entry.path)) {
          items.push({ label, onSelect: () => onPick({ kind: "ignore", pattern }) });
        }
      }
    }
  }
  if (tools !== undefined) items.push(...fileToolItems(entry, onPick, tools));
  return items;
}

/** The external-tool items of a working-tree file's menu (editor, file manager, diff/merge tool). */
export function fileToolItems(
  entry: StatusEntry,
  onPick: (action: FileAction) => void,
  tools: { mergeToolRunning: boolean },
): ContextMenuItem[] {
  const items: ContextMenuItem[] = [];
  const deleted = entry.kind === "Deleted";
  items.push(
    {
      label: "Open in editor",
      disabled: deleted,
      title: deleted ? "The file was deleted" : undefined,
      onSelect: () => onPick({ kind: "openInEditor" }),
    },
    { label: "Reveal in file manager", onSelect: () => onPick({ kind: "reveal" }) },
  );
  if (entry.kind === "Conflicted") {
    items.push({
      label: "Open in merge tool",
      disabled: tools.mergeToolRunning,
      onSelect: () => onPick({ kind: "mergeTool" }),
    });
  } else {
    items.push({ label: "Open in diff tool", onSelect: () => onPick({ kind: "diffTool" }) });
  }
  return items;
}

/** What a multi-file selection holds, for its actions' labels and availability. */
export interface SelectionSummary {
  staged: boolean;
  /** Selected files that are not conflicted: what stage, unstage, discard and stash act on. */
  actionable: number;
  /** Selected untracked files: what "Ignore" acts on. */
  untracked: number;
}

function files(n: number): string {
  return `${n} ${n === 1 ? "file" : "files"}`;
}

/**
 * The actions of a multi-file selection (#449), for the selection bar and the menu of a selected
 * row. `busyReason` (another action is running) disables the repository actions; `stashReason`
 * (`stashBlockReason`) disables stashing as it does "Stash file…".
 */
export function selectionMenuItems(
  summary: SelectionSummary,
  onPick: (action: FileAction) => void,
  busyReason: string | null,
  stashReason: string | null,
): ContextMenuItem[] {
  const n = summary.actionable;
  const blocked = (extra: boolean, reason: string | null = busyReason) => ({
    disabled: reason !== null || extra,
    title: reason ?? undefined,
  });
  const items: ContextMenuItem[] = [];
  if (summary.staged) {
    items.push({ label: `Unstage ${files(n)}`, ...blocked(n === 0), onSelect: () => onPick({ kind: "unstageSelection" }) });
  } else {
    items.push(
      { label: `Stage ${files(n)}`, ...blocked(n === 0), onSelect: () => onPick({ kind: "stageSelection" }) },
      {
        label: `Discard ${files(n)}…`,
        destructive: true,
        ...blocked(n === 0),
        onSelect: () => onPick({ kind: "discardSelection" }),
      },
    );
    if (summary.untracked > 0) {
      items.push({
        label: `Ignore ${summary.untracked} untracked ${summary.untracked === 1 ? "file" : "files"}`,
        ...blocked(false),
        onSelect: () => onPick({ kind: "ignoreSelection" }),
      });
    }
  }
  items.push({
    label: `Stash ${files(n)}…`,
    ...blocked(n === 0, stashReason ?? busyReason),
    onSelect: () => onPick({ kind: "stashSelection" }),
  });
  return items;
}

/** The menu of a read-only file section (a commit's or a compare's file). */
export function commitFileMenuItems(onPick: (action: FileAction) => void): ContextMenuItem[] {
  return [
    { label: "Open in editor", onSelect: () => onPick({ kind: "openInEditor" }) },
    { label: "Reveal in file manager", onSelect: () => onPick({ kind: "reveal" }) },
    { label: "Open in diff tool", onSelect: () => onPick({ kind: "diffTool" }) },
  ];
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
