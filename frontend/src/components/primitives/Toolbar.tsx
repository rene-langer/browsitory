import type { ReactNode } from "react";
import styles from "./Toolbar.module.css";

/**
 * A styled flex row that groups related controls.
 *
 * Deliberately carries no ARIA role. It used to render `role="toolbar"`, but it is applied
 * per-row across list-rendering panels (one per branch, remote, worktree, submodule, tag and
 * reflog entry), so a busy repository emitted hundreds of unnamed toolbars — none of which
 * implemented the roving-tabindex/arrow-key navigation that the ARIA toolbar role promises.
 * Claiming a role without its expected keyboard behaviour is worse than claiming none, so this
 * is purely a layout wrapper; the buttons inside stay individually tabbable, which is the
 * behaviour users actually get.
 */
export function Toolbar({ children }: { children: ReactNode }) {
  return <div className={styles.toolbar}>{children}</div>;
}
