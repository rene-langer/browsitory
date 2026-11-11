import { useEffect, useState } from "react";
import { CircleAlert, CircleCheck, CircleX, Info, X, type LucideIcon } from "lucide-react";
import { toastActions, type Toast, type ToastTone } from "../../state/useToasts";
import styles from "./ToastRegion.module.css";

// Each tone has its own icon shape, so the tone never rests on colour alone.
const TONE_ICON: Record<ToastTone, LucideIcon> = {
  success: CircleCheck,
  info: Info,
  warning: CircleAlert,
  error: CircleX,
};

function ToneIcon({ tone }: { tone: ToastTone }) {
  const Icon = TONE_ICON[tone];
  return <Icon size={16} className={styles.icon} aria-hidden="true" />;
}

// Read out before the message.
const TONE_PREFIX: Record<ToastTone, string | null> = {
  success: null,
  info: null,
  warning: "Warning: ",
  error: "Error: ",
};

/**
 * Polite live region for transient messages (`useToasts`). Always mounted (even when empty) so
 * screen readers register the region before the first message arrives. Fixed-position, so showing
 * a toast never shifts the layout. A toast never takes focus; its buttons are in the Tab order,
 * and every toast's clock stops while the pointer is over one or focus is in one.
 */
export function ToastRegion({
  toasts,
  onDismiss,
  onPauseChange,
}: {
  toasts: Toast[];
  onDismiss: (id: number) => void;
  onPauseChange: (paused: boolean) => void;
}) {
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  // A toast removed under the pointer or with focus inside fires no leave or blur, so the last
  // one going resets both.
  const empty = toasts.length === 0;
  const [wasEmpty, setWasEmpty] = useState(empty);
  if (empty !== wasEmpty) {
    setWasEmpty(empty);
    if (empty) {
      setHovered(false);
      setFocused(false);
    }
  }
  const paused = !empty && (hovered || focused);
  useEffect(() => {
    onPauseChange(paused);
  }, [paused, onPauseChange]);

  return (
    <div
      role="status"
      aria-live="polite"
      aria-label="Notifications"
      className={styles.region}
      onFocus={() => setFocused(true)}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocused(false);
      }}
    >
      {toasts.map((toast) => (
        <div
          key={toast.id}
          data-testid="toast"
          data-tone={toast.tone}
          className={`${styles.toast} ${styles[toast.tone]}`}
          onMouseEnter={() => setHovered(true)}
          onMouseLeave={() => setHovered(false)}
        >
          <ToneIcon tone={toast.tone} />
          <span>
            {TONE_PREFIX[toast.tone] !== null && <span className={styles.srOnly}>{TONE_PREFIX[toast.tone]}</span>}
            {toast.message}
          </span>
          {toastActions(toast).map((action) => (
            <button
              key={action.label}
              type="button"
              className={styles.action}
              onClick={() => {
                action.run();
                onDismiss(toast.id);
              }}
            >
              {action.label}
            </button>
          ))}
          <button type="button" className={styles.dismiss} aria-label="Dismiss notification" onClick={() => onDismiss(toast.id)}>
            <X size={14} aria-hidden="true" />
          </button>
        </div>
      ))}
    </div>
  );
}
