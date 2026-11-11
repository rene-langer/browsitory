import { useEffect, useRef, type ReactNode } from "react";
import { useRestoreFocus } from "./useRestoreFocus";
import styles from "./FormDialog.module.css";

/**
 * Modal `<dialog>` shell for small forms and multi-button prompts. Owns the mechanics BranchTree's
 * hand-rolled dialogs each reimplemented: `showModal()` (or the `open` attribute where
 * unsupported), Escape (`cancel` event) routed to `onCancel` instead of letting the browser close
 * the element behind React's state, and initial focus on an element marked `data-autofocus`, else
 * the first form field, else the first button. Callers render it conditionally — mounting is
 * opening, unmounting is closing (same model as `ConfirmDialog`). The `aria-label` sits on the
 * `<dialog>` itself so both tests and E2E specs can address it as `dialog[aria-label=...]`.
 */
export function FormDialog({
  ariaLabel,
  onCancel,
  returnFocusTo,
  className,
  children,
}: {
  ariaLabel: string;
  /** Added to the dialog's own class, e.g. for a dialog that needs another size. */
  className?: string;
  onCancel: () => void;
  /** Focused on close instead of the element focused when the dialog opened. */
  returnFocusTo?: HTMLElement | null;
  children: ReactNode;
}) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  useRestoreFocus(returnFocusTo);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog === null) return;
    if (!dialog.open && typeof dialog.showModal === "function") {
      dialog.showModal();
    } else if (!dialog.open) {
      dialog.setAttribute("open", "");
    }
    const target =
      dialog.querySelector<HTMLElement>("[data-autofocus]") ??
      dialog.querySelector<HTMLElement>("input, select, textarea") ??
      dialog.querySelector<HTMLElement>("button");
    target?.focus();
  }, []);

  return (
    <dialog
      ref={dialogRef}
      className={className === undefined ? styles.dialog : `${styles.dialog} ${className}`}
      aria-label={ariaLabel}
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
    >
      {children}
    </dialog>
  );
}
