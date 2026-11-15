import type { ReactNode } from "react";
import styles from "./DialogActions.module.css";

/**
 * A dialog's button row (#987): every dialog's buttons at the right end of one row, the primary
 * action first and Cancel last. Mark the primary button `data-tone="primary"` (the accent fill
 * the Commit button uses) or, where it loses data, `data-tone="danger"` (#988); the tones are
 * app-wide (`index.css`), and a danger button here is filled.
 */
export function DialogActions({ children }: { children: ReactNode }) {
  return <div className={styles.actions}>{children}</div>;
}
