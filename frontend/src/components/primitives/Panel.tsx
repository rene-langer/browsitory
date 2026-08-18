import type { ReactNode } from "react";
import styles from "./Panel.module.css";

export function Panel({
  title,
  actions,
  children,
  ariaLive,
  ariaLabel,
}: {
  title?: string;
  actions?: ReactNode;
  children: ReactNode;
  ariaLive?: "polite" | "assertive";
  ariaLabel?: string;
}) {
  return (
    <section className={styles.panel} aria-live={ariaLive} aria-label={ariaLabel ?? title}>
      {title !== undefined && (
        <header className={styles.header}>
          <h2 className={styles.title}>{title}</h2>
          {actions !== undefined && <div className={styles.actions}>{actions}</div>}
        </header>
      )}
      <div className={styles.body}>{children}</div>
    </section>
  );
}
