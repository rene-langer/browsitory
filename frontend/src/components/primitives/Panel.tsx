import { createElement, type ReactNode } from "react";
import styles from "./Panel.module.css";

export function Panel({
  title,
  actions,
  children,
  ariaLive,
  ariaLabel,
  headingLevel = 2,
}: {
  title?: string;
  actions?: ReactNode;
  children: ReactNode;
  ariaLive?: "polite" | "assertive";
  ariaLabel?: string;
  /** Which heading level to render `title` at — default `2`. Pass `3` (or higher) when this
   * `Panel` nests inside another element that already owns an `<h2>` (e.g. an `AccordionSection`
   * body), so the document outline doesn't jump straight from one `<h2>` to a sibling `<h2>`. */
  headingLevel?: 2 | 3 | 4 | 5 | 6;
}) {
  return (
    <section className={styles.panel} aria-live={ariaLive} aria-label={ariaLabel ?? title}>
      {title !== undefined && (
        <header className={styles.header}>
          {createElement(`h${headingLevel}`, { className: styles.title }, title)}
          {actions !== undefined && <div className={styles.actions}>{actions}</div>}
        </header>
      )}
      <div className={styles.body}>{children}</div>
    </section>
  );
}
