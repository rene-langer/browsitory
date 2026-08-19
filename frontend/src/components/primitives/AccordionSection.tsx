import { ChevronDown, ChevronRight } from "lucide-react";
import { useState, type ReactNode } from "react";
import styles from "./AccordionSection.module.css";

function loadOpen(storageKey: string, defaultOpen: boolean): boolean {
  const stored = localStorage.getItem(storageKey);
  if (stored === "open") return true;
  if (stored === "closed") return false;
  return defaultOpen;
}

export function AccordionSection({
  title,
  storageKey,
  defaultOpen = false,
  children,
}: {
  title: string;
  storageKey: string;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(() => loadOpen(storageKey, defaultOpen));

  function toggle() {
    setOpen((prev) => {
      const next = !prev;
      localStorage.setItem(storageKey, next ? "open" : "closed");
      return next;
    });
  }

  return (
    <section className={styles.section} aria-label={title}>
      {/* WAI-ARIA APG accordion pattern: a heading wrapping the trigger
          button, so the section carries the heading role Panel used to. */}
      <h2 className={styles.heading}>
        <button type="button" className={styles.header} aria-expanded={open} onClick={toggle}>
          {open ? (
            <ChevronDown size={14} aria-hidden="true" />
          ) : (
            <ChevronRight size={14} aria-hidden="true" />
          )}
          <span className={styles.title}>{title}</span>
        </button>
      </h2>
      {open && <div className={styles.body}>{children}</div>}
    </section>
  );
}
