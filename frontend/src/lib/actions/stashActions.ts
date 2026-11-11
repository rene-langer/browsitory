import type { StatusEntry, StashEntry } from "../../ipc/RepoClient";
import { busyReason, operationBlockers, type OperationState } from "../operationStatus";
import { blockedBy, gate, type PaletteAction } from "./action";
import { formatRelativeDate } from "../formatDate";

/** Why a stored stash's Apply/Pop and Drop can't run now, `null` when they can. */
export interface StashBlockers {
  apply: string | null;
  drop: string | null;
}

/**
 * The rules for a stored stash's actions (#509), shared by the Stashes panel, the stash view and
 * the command palette:
 *
 * - Apply and Pop: libgit2 refuses while the index has conflicts or differs from HEAD, so they say
 *   what to do instead; otherwise the one busy rule (`operationBlockers`).
 * - Drop: only an action in flight or a transfer blocks it (`busyReason`, #521); dropping a stash
 *   while resolving a paused merge, rebase or sequence step is fine.
 */
export function stashBlockers(state: OperationState & { status: readonly Pick<StatusEntry, "staged" | "kind">[] }): StashBlockers {
  const apply = state.status.some((entry) => entry.kind === "Conflicted")
    ? "Resolve conflicts first."
    : state.status.some((entry) => entry.staged)
      ? "Unstage or commit staged changes first."
      : operationBlockers(state).reason;
  return { apply, drop: busyReason(state) };
}

export interface StashActions {
  /** Apply and Pop restore the stash's staged changes as staged (`git stash apply --index`). */
  apply: PaletteAction;
  pop: PaletteAction;
  /**
   * Apply and Pop leaving everything unstaged, offered only for a stash that recorded staged
   * changes (`hasStagedChanges`): for any other stash the two are the same (#657).
   */
  applyUnstaged: PaletteAction | null;
  popUnstaged: PaletteAction | null;
  /** Opens the confirmation (in the palette: shows the stash and opens the Stashes section). */
  drop: PaletteAction;
}

/** Availability, reason and labels of a stored stash's actions (see `stashBlockers`). */
export function stashActions(blockers: StashBlockers, stash: StashEntry): StashActions {
  const apply = gate(blockedBy(blockers.apply));
  const title = stashTitle(stash);
  return {
    apply: { label: "Apply", paletteLabel: `Apply stash: ${title}`, ...apply },
    pop: { label: "Pop", paletteLabel: `Pop stash: ${title}`, ...apply },
    applyUnstaged: stash.hasStagedChanges
      ? { label: "Apply as unstaged", paletteLabel: `Apply stash as unstaged: ${title}`, ...apply }
      : null,
    popUnstaged: stash.hasStagedChanges
      ? { label: "Pop as unstaged", paletteLabel: `Pop stash as unstaged: ${title}`, ...apply }
      : null,
    drop: { label: "Drop…", paletteLabel: `Drop stash: ${title}`, ...gate(blockedBy(blockers.drop)) },
  };
}

/**
 * A stash's name without git's branch prefix, which `stashDetails` shows: the message given when
 * stashing (`On main: fix tests` → `fix tests`), or for git's default message the commit it was
 * made on (`WIP on main: abc1234 Add` → `WIP: abc1234 Add`). A message in another form is kept.
 */
export function stashTitle(stash: Pick<StashEntry, "message" | "branch">): string {
  if (stash.branch === null) return stash.message;
  for (const [prefix, shown] of [
    [`On ${stash.branch}: `, ""],
    [`WIP on ${stash.branch}: `, "WIP: "],
  ] as const) {
    if (stash.message.startsWith(prefix)) {
      const rest = stash.message.slice(prefix.length);
      if (rest.trim() !== "") return shown + rest;
    }
  }
  return stash.message;
}

/** "On main · 2 days ago · 3 files" (#657): the branch, the stash's age and its file count. */
export function stashDetails(stash: Pick<StashEntry, "branch" | "timestamp" | "fileCount">, nowMs?: number): string {
  const parts: string[] = [];
  if (stash.branch !== null) parts.push(stash.branch === "(no branch)" ? "Detached HEAD" : `On ${stash.branch}`);
  parts.push(formatRelativeDate(stash.timestamp, nowMs));
  parts.push(stash.fileCount === 1 ? "1 file" : `${stash.fileCount} files`);
  return parts.join(" · ");
}
