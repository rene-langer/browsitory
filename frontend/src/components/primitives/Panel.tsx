import { createElement, type ReactNode } from "react";
import styles from "./Panel.module.css";

export function Panel({
  title,
  actions,
  children,
  ariaLive,
  ariaLabel,
  headingLevel = 2,
  footer,
  bodyLabel,
  bodyClassName,
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
  /** Controls (e.g. a dialog's Close button) that stay in place below the body. With a footer
   * the panel fits the height it is given (an `Overlay`'s) and only the body scrolls, so the
   * footer is always visible and the dialog's initial focus never scrolls the body (#378, #379). */
  footer?: ReactNode;
  /** With `footer`: makes the scrolling body a labelled, focusable region, so keyboard users can
   * scroll it. Being the first focusable element, it then also takes a dialog's initial focus. */
  bodyLabel?: string;
  /** With `footer`: an extra class for the scrolling body, e.g. to lay it out as a flex column
   * whose one child fills the height left and scrolls on its own (#383). */
  bodyClassName?: string;
}) {
  const scrolling = footer !== undefined;
  return (
    <section
      className={scrolling ? `${styles.panel} ${styles.withFooter}` : styles.panel}
      aria-live={ariaLive}
      aria-label={ariaLabel ?? title}
    >
      {title !== undefined && (
        <header className={styles.header}>
          {createElement(`h${headingLevel}`, { className: styles.title }, title)}
          {actions !== undefined && <div className={styles.actions}>{actions}</div>}
        </header>
      )}
      {scrolling ? (
        <>
          <div
            className={[styles.body, styles.scrollBody, bodyClassName].filter(Boolean).join(" ")}
            {...(bodyLabel !== undefined ? { role: "region", "aria-label": bodyLabel, tabIndex: 0 } : {})}
          >
            {children}
          </div>
          <footer className={styles.footer}>{footer}</footer>
        </>
      ) : (
        <div className={styles.body}>{children}</div>
      )}
    </section>
  );
}
