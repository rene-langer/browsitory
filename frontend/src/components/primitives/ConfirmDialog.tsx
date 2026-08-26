import { useEffect, useRef, type ReactNode } from "react";
import styles from "./ConfirmDialog.module.css";

/**
 * Shared confirmation UI for destructive actions (remove worktree, remove remote, force-delete
 * branch, …). Owns the `<dialog>` mechanics every call site used to reimplement slightly
 * differently: `showModal()`/`open`-attribute wiring, Escape-key (`cancel` event) handling that
 * routes back through `onCancel` instead of letting the browser close the element out from under
 * React state, and autofocusing the safe/cancel action so a stray Enter never confirms.
 *
 * Callers render this conditionally (`X !== null && <ConfirmDialog ... />`) rather than passing
 * an `open` prop — mounting *is* opening, unmounting *is* closing, matching how every existing
 * call site already models its confirmation state.
 */
export function ConfirmDialog({
  ariaLabel,
  message,
  confirmLabel,
  cancelLabel = "Cancel",
  onConfirm,
  onCancel,
  confirmDisabled = false,
}: {
  /** Accessible name for the dialog (`aria-label`). */
  ariaLabel: string;
  /** Body content — a plain string or richer markup (e.g. a `<p>` naming the affected branches). */
  message: ReactNode;
  confirmLabel: string;
  cancelLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
  confirmDisabled?: boolean;
}) {
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog === null) return;
    if (!dialog.open && typeof dialog.showModal === "function") {
      dialog.showModal();
    } else if (!dialog.open) {
      dialog.setAttribute("open", "");
    }
    dialog.querySelector<HTMLButtonElement>("[data-autofocus]")?.focus();
  }, []);

  return (
    <dialog
      ref={dialogRef}
      className={styles.dialog}
      aria-label={ariaLabel}
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
    >
      <div className={styles.message}>{message}</div>
      <div className={styles.actions}>
        <button type="button" className={styles.confirmButton} disabled={confirmDisabled} onClick={onConfirm}>
          {confirmLabel}
        </button>
        <button type="button" data-autofocus onClick={onCancel}>
          {cancelLabel}
        </button>
      </div>
    </dialog>
  );
}
