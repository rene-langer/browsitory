import type { Toast } from "../../state/useToasts";
import styles from "./ToastRegion.module.css";

/**
 * Polite live region for transient success messages. Always mounted (even when empty) so screen
 * readers register the region before the first message arrives. Fixed-position, so showing a
 * toast never shifts the layout.
 */
export function ToastRegion({ toasts, onDismiss }: { toasts: Toast[]; onDismiss: (id: number) => void }) {
  return (
    <div role="status" aria-live="polite" aria-label="Notifications" className={styles.region}>
      {toasts.map((toast) => (
        <div key={toast.id} className={styles.toast}>
          <span>{toast.message}</span>
          {toast.action !== undefined && (
            <button
              type="button"
              className={styles.action}
              onClick={() => {
                toast.action?.run();
                onDismiss(toast.id);
              }}
            >
              {toast.action.label}
            </button>
          )}
          <button type="button" aria-label="Dismiss notification" onClick={() => onDismiss(toast.id)}>
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
