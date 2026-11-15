import type { ReactNode } from "react";
import styles from "./DialogHeader.module.css";

/**
 * A dialog's visible title (#987): one bold heading, the same in every dialog, with an optional
 * line under it (what the dialog acts on, a count). `FormDialog` and `ConfirmDialog` render it
 * from their `title`, which also names the dialog, so the name a screen reader announces is the
 * text on screen.
 */
export function DialogHeader({ title, description, id }: { title: string; description?: ReactNode; id?: string }) {
  return (
    <header className={styles.header}>
      <h2 id={id} className={styles.title}>
        {title}
      </h2>
      {description !== undefined && description !== null && <div className={styles.description}>{description}</div>}
    </header>
  );
}
