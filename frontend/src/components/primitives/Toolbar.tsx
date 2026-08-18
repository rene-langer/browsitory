import type { ReactNode } from "react";
import styles from "./Toolbar.module.css";

export function Toolbar({ children }: { children: ReactNode }) {
  return (
    <div className={styles.toolbar} role="toolbar">
      {children}
    </div>
  );
}
