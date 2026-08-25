import { useRef, type ReactNode } from "react";
import { ChevronsDownUp, ChevronsUpDown } from "lucide-react";
import { AccordionGroup, type AccordionGroupHandle } from "./AccordionGroup";
import styles from "./Sidebar.module.css";

export function Sidebar({ children }: { children: ReactNode }) {
  const groupRef = useRef<AccordionGroupHandle | null>(null);

  return (
    <aside className={styles.sidebar} aria-label="Repository sections">
      <div className={styles.toolbar}>
        <button
          type="button"
          className={styles.toolbarButton}
          aria-label="Expand all sections"
          onClick={() => groupRef.current?.expandAll()}
        >
          <ChevronsUpDown size={14} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={styles.toolbarButton}
          aria-label="Collapse all sections"
          onClick={() => groupRef.current?.collapseAll()}
        >
          <ChevronsDownUp size={14} aria-hidden="true" />
        </button>
      </div>
      <AccordionGroup groupRef={groupRef}>
        <div className={styles.sections}>{children}</div>
      </AccordionGroup>
    </aside>
  );
}
