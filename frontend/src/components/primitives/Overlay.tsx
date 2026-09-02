import { useEffect, useRef, type ReactNode } from "react";
import styles from "./Overlay.module.css";

export function Overlay({ onClose, children }: { onClose?: () => void; children: ReactNode }) {
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

  return (
    <dialog ref={ref} className={styles.overlay} onClose={handleClose}>
      {children}
    </dialog>
  );
}
