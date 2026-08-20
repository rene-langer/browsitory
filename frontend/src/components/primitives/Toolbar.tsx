import type { ReactNode } from "react";
import styles from "./Toolbar.module.css";

/**
 * A styled flex row that groups related controls.
 *
 * Deliberately never renders `role="toolbar"`. It used to, but that role is applied per-row
 * across list-rendering panels (one per branch, remote, worktree, submodule, tag and reflog
 * entry), so a busy repository emitted hundreds of unnamed toolbars — none of which implemented
 * the roving-tabindex/arrow-key navigation the ARIA toolbar role promises. Claiming a role
 * without its expected keyboard behaviour is worse than claiming none.
 *
 * Renders `role="group"` instead — a plain grouping semantic with no keyboard-behaviour contract
 * to break — plus an optional `aria-label` a caller can pass when a specific instance is
 * genuinely ambiguous without one. The buttons inside stay individually tabbable either way,
 * which is the behaviour users actually get.
 */
export function Toolbar({ "aria-label": ariaLabel, children }: { "aria-label"?: string; children: ReactNode }) {
  return (
    <div className={styles.toolbar} role="group" aria-label={ariaLabel}>
      {children}
    </div>
  );
}
