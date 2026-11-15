import { useEffect, useId, useRef, type ReactNode } from "react";
import { BlockedReason } from "./BlockedReason";
import { DialogActions } from "./DialogActions";
import { DialogHeader } from "./DialogHeader";
import { describedBy } from "../../lib/ariaDescribedBy";
import { useRestoreFocus } from "./useRestoreFocus";
import styles from "./ConfirmDialog.module.css";

/**
 * Shared confirmation UI (remove worktree, remove remote, force-delete branch, …). Owns the
 * `<dialog>` mechanics every call site used to reimplement slightly differently:
 * `showModal()`/`open`-attribute wiring, Escape-key (`cancel` event) handling that routes back
 * through `onCancel` instead of letting the browser close the element out from under React state,
 * and autofocusing the safe/cancel action so a stray Enter never confirms.
 *
 * Callers render this conditionally (`X !== null && <ConfirmDialog ... />`) rather than passing
 * an `open` prop — mounting *is* opening, unmounting *is* closing, matching how every existing
 * call site already models its confirmation state.
 *
 * `title` is the visible heading and the dialog's name (#987); `message` says what happens and
 * whether Undo brings it back (#989). The confirm button is the primary action; `tone="danger"`
 * marks it as one that loses data (#988), and only then.
 */
export function ConfirmDialog({
  title,
  message,
  confirmLabel,
  cancelLabel = "Cancel",
  tone = "default",
  onConfirm,
  onCancel,
  confirmDisabled = false,
  confirmTitle,
  confirmReason = null,
  hideConfirm = false,
}: {
  /** The visible title and accessible name (`aria-label`). */
  title: string;
  /** Body content — a plain string or richer markup (e.g. a `<p>` naming the affected branches). */
  message: ReactNode;
  confirmLabel: string;
  cancelLabel?: string;
  /** `"danger"` where confirming loses data (#988); else the confirm button is a plain primary. */
  tone?: "default" | "danger";
  onConfirm: () => void;
  onCancel: () => void;
  confirmDisabled?: boolean;
  /** `title` for the confirm button — e.g. explaining why it's disabled (issue #31/UX-003). */
  confirmTitle?: string;
  /**
   * Why the confirm button can't run now (#748): shown under the message, tied to the button with
   * `aria-describedby`, and the button is disabled. `null` leaves it to `confirmDisabled`.
   */
  confirmReason?: string | null;
  /** Omits the confirm button: an informational dialog whose only action is `cancelLabel`. */
  hideConfirm?: boolean;
}) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const reasonId = useId();
  const titleId = useId();
  const messageId = useId();
  useRestoreFocus();

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
      aria-label={title}
      aria-labelledby={titleId}
      aria-describedby={messageId}
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
    >
      <DialogHeader title={title} id={titleId} />
      <div id={messageId} className={styles.message}>
        {message}
        <BlockedReason id={reasonId} reason={confirmReason} />
      </div>
      <DialogActions>
        {!hideConfirm && (
          <button
            type="button"
            data-tone={tone === "danger" ? "danger" : "primary"}
            disabled={confirmDisabled || confirmReason !== null}
            title={confirmTitle ?? confirmReason ?? undefined}
            aria-describedby={describedBy(reasonId, confirmReason)}
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        )}
        <button type="button" data-autofocus onClick={onCancel}>
          {cancelLabel}
        </button>
      </DialogActions>
    </dialog>
  );
}
