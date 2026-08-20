import type { ReactNode } from "react";
import styles from "./Sidebar.module.css";

export function Sidebar({ children }: { children: ReactNode }) {
  return (
    <aside className={styles.sidebar} aria-label="Repository sections">
      {children}
    </aside>
  );
}
