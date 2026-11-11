import type { OperationBlockers } from "../operationStatus";

/**
 * One action as a menu, a panel button or the command palette offers it (#495, #509). Each entity
 * has its own module (`commitActions`, `branchActions`, …) that decides the labels and whether the
 * action can run, so the mouse and the keyboard paths get the same answer.
 */
export interface MenuAction {
  /** The menu's or button's label ("Fetch"). */
  label: string;
  /** The menu disables the item, the palette leaves the command out. */
  enabled: boolean;
  /** Why it is disabled, for the item's title; `undefined` while it is enabled. */
  reason: string | undefined;
}

/** An action the command palette offers too, under a label that names its target ("Fetch origin"). */
export interface PaletteAction extends MenuAction {
  paletteLabel: string;
}

/** What an unavailable action says when it gives no reason of its own. */
export const UNAVAILABLE_REASON = "Not available right now.";

/** Why `action` can't run, for a `ReasonButton` (#748); `null` while it is enabled. */
export function disabledReason(action: Pick<MenuAction, "enabled" | "reason">): string | null {
  return action.enabled ? null : (action.reason ?? UNAVAILABLE_REASON);
}

/**
 * Enabled unless `blockers` (e.g. `operationBlockers`) block it or `refusal` (why this target
 * can't take the action) says otherwise; the busy reason wins.
 */
export function gate(blockers: OperationBlockers, refusal?: string): Pick<MenuAction, "enabled" | "reason"> {
  if (blockers.blocked) return { enabled: false, reason: blockers.reason ?? undefined };
  return { enabled: refusal === undefined, reason: refusal };
}

/** A narrower busy rule's reason (`busyReason`, `stashBlockReason`) as `OperationBlockers`. */
export function blockedBy(reason: string | null): OperationBlockers {
  return { blocked: reason !== null, reason };
}

/** A context menu item for `action` (a `ContextMenuItem`): disabled with its reason as the title. */
export function menuItem(
  action: MenuAction,
  onSelect: () => void,
): { label: string; disabled: boolean; title: string | undefined; onSelect: () => void } {
  return { label: action.label, disabled: !action.enabled, title: action.reason, onSelect };
}
