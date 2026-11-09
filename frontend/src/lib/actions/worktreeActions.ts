import type { WorktreeInfo } from "../../ipc/RepoClient";
import type { OperationBlockers } from "../operationStatus";
import { gate, type MenuAction, type PaletteAction } from "./action";

export const MAIN_WORKTREE = "The main worktree can't be removed.";

export interface WorktreeActions {
  /** Opens the worktree in a tab of its own. */
  open: PaletteAction;
  /** Opens the confirmation (in the palette: opens the Worktrees section). */
  remove: PaletteAction;
}

/**
 * Availability, reason and labels of a worktree's actions (#509), shared by the Worktrees panel
 * and the command palette; both follow the one busy rule (`operationBlockers`). The palette lists
 * linked worktrees only.
 */
export function worktreeActions(blockers: OperationBlockers, worktree: WorktreeInfo): WorktreeActions {
  return {
    open: { label: `Open ${worktree.path}`, paletteLabel: `Open worktree ${worktree.name}`, ...gate(blockers) },
    remove: {
      label: `Remove ${worktree.path}`,
      paletteLabel: `Remove worktree ${worktree.name}`,
      ...gate(blockers, worktree.isMain ? MAIN_WORKTREE : undefined),
    },
  };
}

/** The Worktrees panel's "Prune worktrees" button and the palette's command of that name. */
export function pruneWorktreesAction(blockers: OperationBlockers): MenuAction {
  return { label: "Prune worktrees", ...gate(blockers) };
}
