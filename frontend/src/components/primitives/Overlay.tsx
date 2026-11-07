import { useEffect, useRef, type MouseEvent, type ReactNode } from "react";
import styles from "./Overlay.module.css";

export function Overlay({
  onClose,
  ariaLabel,
  closeOnBackdropClick = false,
  children,
}: {
  onClose?: () => void;
  /** The `<dialog>`'s accessible name. Give one whenever the overlay is a dialog of its own. */
  ariaLabel?: string;
  /** Closes the overlay (calling `onClose`) on a click on the backdrop. Off by default: a modal
   * `<dialog>` ignores backdrop clicks, and an overlay holding unsaved input shouldn't lose it to
   * a stray click. Meant for read-only overlays such as the release notes. */
  closeOnBackdropClick?: boolean;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const openedViaShowModal = useRef(false);
  // React StrictMode (dev only) double-invokes a fresh mount's effect: setup, then its
  // cleanup, then setup again, to surface non-idempotent effects. Our cleanup calls
  // `dialog.close()`, which fires a real native "close" event — without this guard that
  // event reaches `onClose` and, since `onClose` here is always "flip the state that
  // controls whether this Overlay is mounted", immediately unmounts the Overlay again,
  // so the picker overlay opened via the tab strip's "+" appears to do nothing. This flag
  // suppresses only the "close" event our own cleanup causes, not a real user-driven one
  // (Escape, backdrop click, a `<form method="dialog">` submit).
  const suppressNextClose = useRef(false);

  useEffect(() => {
    const dialog = ref.current;
    if (dialog === null) return;
    if (!dialog.open && typeof dialog.showModal === "function") {
      dialog.showModal();
      openedViaShowModal.current = true;
      // `showModal()` focuses the first focusable control, which scrolls a long dialog to it
      // (e.g. a Close button at the bottom) instead of leaving the top visible. This reset only
      // helps where that scroll happens inside `showModal()` (Chromium). WebKit (the Tauri app
      // on Linux and macOS) scrolls to the focused control later, at its next layout, so the
      // reset can't undo it there (#378): a long overlay must keep its first focusable control
      // in view, e.g. with a `Panel` footer, which keeps the controls out of the scrolling body.
      dialog.scrollTop = 0;
    } else if (!dialog.open) {
      dialog.setAttribute("open", "");
    }
    return () => {
      if (openedViaShowModal.current) {
        suppressNextClose.current = true;
        dialog.close();
      } else {
        dialog.removeAttribute("open");
      }
    };
  }, []);

  const handleClose = () => {
    if (suppressNextClose.current) {
      suppressNextClose.current = false;
      return;
    }
    onClose?.();
  };

  // A press on the backdrop is dispatched to the `<dialog>` itself, as is one on its border, so
  // the position tells the two apart. Presses on the content target a child.
  const onBackdrop = (event: MouseEvent<HTMLDialogElement>) => {
    if (event.target !== event.currentTarget) return false;
    const rect = event.currentTarget.getBoundingClientRect();
    const inside =
      event.clientX >= rect.left &&
      event.clientX <= rect.right &&
      event.clientY >= rect.top &&
      event.clientY <= rect.bottom;
    return !inside;
  };
  // Where the last press started. A drag that starts in the content (e.g. selecting text) and
  // ends over the backdrop also fires a click on the `<dialog>`, their common ancestor, at a
  // point outside it; it must not close the overlay.
  const pressStartedOnBackdrop = useRef(false);
  const handleMouseDown = (event: MouseEvent<HTMLDialogElement>) => {
    pressStartedOnBackdrop.current = closeOnBackdropClick && onBackdrop(event);
  };
  const handleClick = (event: MouseEvent<HTMLDialogElement>) => {
    const startedOnBackdrop = pressStartedOnBackdrop.current;
    pressStartedOnBackdrop.current = false;
    if (closeOnBackdropClick && startedOnBackdrop && onBackdrop(event)) onClose?.();
  };

  return (
    // The dialog closes on Escape by itself; the mouse handlers only add the backdrop click.
    <dialog
      ref={ref}
      className={styles.overlay}
      aria-label={ariaLabel}
      onClose={handleClose}
      onMouseDown={handleMouseDown}
      onClick={handleClick}
    >
      {children}
    </dialog>
  );
}
