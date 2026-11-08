import { X } from "lucide-react";
import type { GoToNotice as Notice } from "../state/useGoTo";
import styles from "./GoToNotice.module.css";

/**
 * Above the graph: why a "Go to" target has no row in it, with the one change that would show
 * it (#441). A polite status, not an alert: nothing failed, the graph is filtered.
 */
export function GoToNotice({ notice, onDismiss }: { notice: Notice; onDismiss: () => void }) {
  return (
    <p role="status" className={styles.notice}>
      <span className={styles.message}>{notice.message}</span>
      {notice.action !== null && (
        <button type="button" className={styles.action} onClick={notice.action.run}>
          {notice.action.label}
        </button>
      )}
      <button type="button" className={styles.dismiss} aria-label="Dismiss notice" onClick={onDismiss}>
        <X size={12} aria-hidden="true" />
      </button>
    </p>
  );
}
