import type { ReactNode } from "react";
import styles from "./SplitView.module.css";

export function SplitView({ left, right }: { left: ReactNode; right: ReactNode }) {
  return (
    <div className={styles.splitView}>
      <div className={styles.left}>{left}</div>
      <div className={styles.right}>{right}</div>
    </div>
  );
}
