import { X } from "lucide-react";
import styles from "./InlineError.module.css";

/**
 * Shared dismissible error display for both the global banner (`App.tsx`) and every component's
 * own inline error (`RemotePanel`'s `addError`, `DiffPane`'s fetch failures, …). Before this,
 * every call site rendered a bare `<p role="alert">{message}</p>` with no way to clear it short
 * of the next successful action of the same kind — a stale error could sit on screen
 * indefinitely once the user moved on to something else (see issue #30/UX-002). Centralizing the
 * dismiss button here means every error surface gets the same affordance for free, and the same
 * markup instead of a copy-pasted `<button>` per call site.
 */
export function InlineError({
  message,
  onDismiss,
  className,
}: {
  message: string;
  onDismiss: () => void;
  className?: string;
}) {
  return (
    <p role="alert" className={className === undefined ? styles.error : `${styles.error} ${className}`}>
      <span className={styles.message}>{message}</span>
      <button type="button" className={styles.dismiss} aria-label="Dismiss error" onClick={onDismiss}>
        <X size={12} aria-hidden="true" />
      </button>
    </p>
  );
}
