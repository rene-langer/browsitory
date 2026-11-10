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
  | { kind: "stashSelection" }
  // A commit's file (#448): revert or apply its change in the working tree (`stage`: and in the
  // index, #536), save it as a patch.
  | { kind: "revertCommitFile"; stage: boolean }
  | { kind: "applyCommitFile"; stage: boolean }
  | { kind: "saveFilePatch" };

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
      group: "file",
    };
    if (entry.staged) {
      items.push(stash);
    } else {
      items.push(
        { label: "Discard changes…", destructive: true, onSelect: () => onPick({ kind: "discard" }), group: "discard" },
        stash,
      );
      if (isUntracked(entry)) {
        for (const { label, pattern } of ignorePatterns(entry.path)) {
          items.push({ label, onSelect: () => onPick({ kind: "ignore", pattern }), group: "file" });
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
      group: "tools",
    },
    { label: "Reveal in file manager", onSelect: () => onPick({ kind: "reveal" }), group: "tools" },
  );
  if (entry.kind === "Conflicted") {
    items.push({
      label: "Open in merge tool",
      disabled: tools.mergeToolRunning,
      onSelect: () => onPick({ kind: "mergeTool" }),
      group: "tools",
    });
  } else {
    items.push({ label: "Open in diff tool", onSelect: () => onPick({ kind: "diffTool" }), group: "tools" });
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
/** The file list's `s` key stages or unstages the selection (#645: shown in the menu). */
const STAGE_KEY = { group: "stage", shortcut: "s", ariaKeyShortcuts: "S" } as const;

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
    items.push({
      label: `Unstage ${files(n)}`,
      ...blocked(n === 0),
      onSelect: () => onPick({ kind: "unstageSelection" }),
      ...STAGE_KEY,
    });
  } else {
    items.push(
      { label: `Stage ${files(n)}`, ...blocked(n === 0), onSelect: () => onPick({ kind: "stageSelection" }), ...STAGE_KEY },
      {
        label: `Discard ${files(n)}…`,
        destructive: true,
        ...blocked(n === 0),
        onSelect: () => onPick({ kind: "discardSelection" }),
        group: "discard",
      },
    );
    if (summary.untracked > 0) {
      items.push({
        label: `Ignore ${summary.untracked} untracked ${summary.untracked === 1 ? "file" : "files"}`,
        ...blocked(false),
        onSelect: () => onPick({ kind: "ignoreSelection" }),
        group: "file",
      });
    }
  }
  items.push({
    label: `Stash ${files(n)}…`,
    ...blocked(n === 0, stashReason ?? busyReason),
    onSelect: () => onPick({ kind: "stashSelection" }),
    group: "file",
  });
  return items;
}

/**
 * The menu of a read-only file section (a commit's or a compare's file). With `patch` (a commit's
 * file, #448), it starts with Revert / Apply in the working tree, each also "and stage" (#536)
 * (disabled with `blockedReason`
 * while repository actions can't run) and "Save file as patch…"; with `tools`, the external
 * tools follow.
 */
export function commitFileMenuItems(
  onPick: (action: FileAction) => void,
  { tools = true, patch = null }: { tools?: boolean; patch?: { blockedReason: string | null } | null } = {},
): ContextMenuItem[] {
  const items: ContextMenuItem[] = [];
  if (patch !== null) {
    const blocked = { disabled: patch.blockedReason !== null, title: patch.blockedReason ?? undefined };
    items.push(
      { label: "Revert this file", ...blocked, onSelect: () => onPick({ kind: "revertCommitFile", stage: false }), group: "patch" },
      {
        label: "Revert this file and stage",
        ...blocked,
        onSelect: () => onPick({ kind: "revertCommitFile", stage: true }),
        group: "patch",
      },
      {
        label: "Apply this file to the working tree",
        ...blocked,
        onSelect: () => onPick({ kind: "applyCommitFile", stage: false }),
        group: "patch",
      },
      {
        label: "Apply this file and stage",
        ...blocked,
        onSelect: () => onPick({ kind: "applyCommitFile", stage: true }),
        group: "patch",
      },
      { label: "Save file as patch…", onSelect: () => onPick({ kind: "saveFilePatch" }), group: "patch" },
    );
  }
  if (tools) {
    items.push(
      { label: "Open in editor", onSelect: () => onPick({ kind: "openInEditor" }), group: "tools" },
      { label: "Reveal in file manager", onSelect: () => onPick({ kind: "reveal" }), group: "tools" },
      { label: "Open in diff tool", onSelect: () => onPick({ kind: "diffTool" }), group: "tools" },
    );
  }
  return items;
}

// Discards and deletes are undo steps (recorded, or refused rather than run unrecorded), so the
// confirmations say Undo restores them (#662); they still ask because they drop work in one go.
export function discardConfirmMessage(entry: StatusEntry): string {
  return isUntracked(entry)
    ? `Delete untracked file ${entry.path}? Undo can bring it back.`
    : `Discard changes to ${entry.path}? Undo can bring them back.`;
}

/**
 * `pointToDeleteUntracked`: the Changes heading's "Discard all…" names "Delete untracked…" (#622)
 * when untracked files are included, for when only those were meant to go.
 */
export function discardAllConfirmMessage(entries: StatusEntry[], pointToDeleteUntracked = false): string {
  const n = entries.length;
  const m = entries.filter(isUntracked).length;
  const files = `${n} ${n === 1 ? "file" : "files"}`;
  const untracked = m > 0 ? ` (${m} untracked will be deleted)` : "";
  const hint =
    m > 0 && m < n && pointToDeleteUntracked
      ? ' To delete only the untracked files and keep the other changes, use "Delete untracked…".'
      : "";
  return `Discard changes to ${files}${untracked}? Undo can bring them back.${hint}`;
}

/** How many paths the "Delete untracked…" confirmation lists before "and N more" (#622). */
export const DELETE_UNTRACKED_PREVIEW = 5;

/** The "Delete untracked…" confirmation's question (#622); the dialog lists the first paths. */
export function deleteUntrackedConfirmMessage(paths: string[]): string {
  const n = paths.length;
  return n === 1 ? "Delete 1 untracked file? Undo can bring it back." : `Delete ${n} untracked files? Undo can bring them back.`;
}
