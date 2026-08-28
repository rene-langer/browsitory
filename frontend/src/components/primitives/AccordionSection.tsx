import { ChevronRight, type LucideIcon } from "lucide-react";
import { createElement, useEffect, useRef, useState, type ReactNode } from "react";
import { loadPersistedOpen, persistOpen } from "../../lib/persistedOpenState";
import { useAccordionGroup } from "./AccordionGroup";
import styles from "./AccordionSection.module.css";

export function AccordionSection({
  title,
  storageKey,
  defaultOpen = false,
  icon: Icon,
  count,
  headingLevel = 2,
  children,
}: {
  title: string;
  storageKey: string;
  defaultOpen?: boolean;
  icon?: LucideIcon;
  count?: number;
  headingLevel?: 2 | 3;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(() => loadPersistedOpen(storageKey, defaultOpen));
  const headerRef = useRef<HTMLButtonElement>(null);
  const group = useAccordionGroup();

  function setOpenState(next: boolean) {
    setOpen(next);
    persistOpen(storageKey, next);
  }

  useEffect(() => {
    return group?.register({ ref: headerRef, setOpen: setOpenState });
    // Register once on mount, not on every `[group]` identity change — see AccordionGroup.tsx's
    // `isActive` (depends on `activeRef`, so the memoized context value's identity changes on
    // every focus/arrow-key move); keying this effect on `group` churns registration on every
    // such change and desyncs tabIndex from the actually active header.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // `isActive` compares ref identity only; it never dereferences `.current`, so reading it
  // during render is safe. `react-hooks/refs` can't verify that statically.
  // eslint-disable-next-line react-hooks/refs
  const headerTabIndex = group === null ? 0 : group.isActive(headerRef) ? 0 : -1;

  return (
    <section className={styles.section} data-open={open} aria-label={title}>
      {/* WAI-ARIA APG accordion pattern: a heading wrapping the trigger button (so the section
          carries the heading role), plus roving-tabindex keyboard nav across sibling headers via
          `AccordionGroup`. Open state itself is independent per section by design (not mutual
          exclusion) — each section persists its own open/closed state under its own
          `storageKey`, so opening one is not meant to close its siblings. */}
      {createElement(
        `h${headingLevel}`,
        { className: styles.heading },
        <button
          ref={headerRef}
          type="button"
          className={styles.header}
          aria-expanded={open}
          aria-label={title}
          tabIndex={headerTabIndex}
          onFocus={() => group?.onHeaderFocus(headerRef)}
          onKeyDown={(event) => group?.onHeaderKeyDown(event, headerRef)}
          onClick={() => setOpenState(!open)}
        >
          <ChevronRight
            size={14}
            aria-hidden="true"
            className={open ? `${styles.chevron} ${styles.chevronOpen}` : styles.chevron}
          />
          {Icon !== undefined && <Icon size={14} aria-hidden="true" className={styles.icon} />}
          <span className={styles.title}>{title}</span>
          {count !== undefined && (
            <span className={styles.count} aria-hidden="true">
              {count}
            </span>
          )}
        </button>,
      )}
      {open && <div className={styles.body}>{children}</div>}
    </section>
  );
}
