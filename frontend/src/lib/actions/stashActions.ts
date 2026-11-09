import type { StatusEntry, StashEntry } from "../../ipc/RepoClient";
import { busyReason, operationBlockers, type OperationState } from "../operationStatus";
import { blockedBy, gate, type PaletteAction } from "./action";

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
  apply: PaletteAction;
  pop: PaletteAction;
  /** Opens the confirmation (in the palette: shows the stash and opens the Stashes section). */
  drop: PaletteAction;
}

/** Availability, reason and labels of a stored stash's actions (see `stashBlockers`). */
export function stashActions(blockers: StashBlockers, stash: StashEntry): StashActions {
  return {
    apply: { label: "Apply", paletteLabel: `Apply stash: ${stash.message}`, ...gate(blockedBy(blockers.apply)) },
    pop: { label: "Pop", paletteLabel: `Pop stash: ${stash.message}`, ...gate(blockedBy(blockers.apply)) },
    drop: { label: "Drop…", paletteLabel: `Drop stash: ${stash.message}`, ...gate(blockedBy(blockers.drop)) },
  };
}
