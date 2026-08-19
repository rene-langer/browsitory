import { useEffect, useRef, type ReactNode } from "react";
import styles from "./Overlay.module.css";

export function Overlay({ onClose, children }: { onClose?: () => void; children: ReactNode }) {
  const ref = useRef<HTMLDialogElement>(null);
  const openedViaShowModal = useRef(false);

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
        dialog.close();
      } else {
        dialog.removeAttribute("open");
      }
    };
  }, []);

  return (
    <dialog ref={ref} className={styles.overlay} onClose={onClose}>
      {children}
    </dialog>
  );
}
