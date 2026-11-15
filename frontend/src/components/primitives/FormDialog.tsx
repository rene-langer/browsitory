import { useEffect, useRef, type ReactNode } from "react";
import { DialogHeader } from "./DialogHeader";
import { useRestoreFocus } from "./useRestoreFocus";
import styles from "./FormDialog.module.css";

/**
 * Modal `<dialog>` shell for small forms and multi-button prompts. Owns the mechanics BranchTree's
 * hand-rolled dialogs each reimplemented: `showModal()` (or the `open` attribute where
 * unsupported), Escape (`cancel` event) routed to `onCancel` instead of letting the browser close
 * the element behind React's state, and initial focus on an element marked `data-autofocus`, else
 * the first form field, else the first button. Callers render it conditionally — mounting is
 * opening, unmounting is closing (same model as `ConfirmDialog`).
 *
 * `title` is the dialog's visible heading (`DialogHeader`, #987) and its accessible name: the
 * `aria-label` on the `<dialog>` is the same text, so tests and E2E specs can address it as
 * `dialog[aria-label=...]`. Put the buttons in `DialogActions`.
 */
/**
 * Where a dialog's focus starts (#1029): a `data-autofocus` element, else its first enabled form
 * field (a radio group's checked radio, where Tab would enter the group), else its first enabled
 * button. Never the frame itself.
 */
function initialFocusTarget(dialog: HTMLElement): HTMLElement | null {
  const marked = dialog.querySelector<HTMLElement>("[data-autofocus]");
  if (marked !== null) return marked;
  const enabled = (selector: string) =>
    [...dialog.querySelectorAll<HTMLInputElement>(selector)].filter((field) => !field.disabled && field.type !== "hidden");
  const field = enabled("input, select, textarea")[0];
  if (field !== undefined) {
    if (field.type !== "radio" || field.name === "") return field;
    const group = enabled(`input[type="radio"][name="${CSS.escape(field.name)}"]`);
    return group.find((radio) => radio.checked) ?? field;
  }
  return enabled("button")[0] ?? null;
}

export function FormDialog({
  title,
  description,
  onCancel,
  returnFocusTo,
  className,
  children,
}: {
  title: string;
  /** A line under the title: what the dialog acts on, a count. */
  description?: ReactNode;
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
    initialFocusTarget(dialog)?.focus();
  }, []);

  return (
    <dialog
      ref={dialogRef}
      className={className === undefined ? styles.dialog : `${styles.dialog} ${className}`}
      aria-label={title}
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
    >
      <DialogHeader title={title} description={description} />
      {children}
    </dialog>
  );
}
