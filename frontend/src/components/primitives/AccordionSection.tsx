import { ChevronRight, type LucideIcon } from "lucide-react";
import { createElement, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { loadPersistedOpen, persistOpen } from "../../lib/persistedOpenState";
import { scrollBehavior } from "../../lib/scrollBehavior";
import { useAccordionGroup } from "./AccordionGroup";
import styles from "./AccordionSection.module.css";

export function AccordionSection({
  title,
  storageKey,
  defaultOpen = false,
  icon: Icon,
  count,
  headingLevel = 2,
  actions,
  revealRequest,
  children,
}: {
  title: string;
  storageKey: string;
  defaultOpen?: boolean;
  icon?: LucideIcon;
  count?: number;
  headingLevel?: 2 | 3;
  /** Controls shown at the right of the header row, beside (not inside) the toggle button. */
  actions?: ReactNode;
  /**
   * A reveal request (`useSidebarReveal`, #494): each new value expands the section and scrolls it
   * into view. The value it mounts with is not a request.
   */
  revealRequest?: number;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(() => loadPersistedOpen(storageKey, defaultOpen));
  const headerRef = useRef<HTMLButtonElement>(null);
  const sectionRef = useRef<HTMLElement>(null);
  const headerRowRef = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);

  // Room for the content (#1010): an open section's minimum height is its header plus its
  // content, up to a cap (`--section-cap` in the CSS), so an open neighbour can't squeeze it to a
  // sliver; past the cap the body scrolls, and `data-fade` shades the edge it cuts. Written to the
  // DOM directly, not to state: the content changes size without this component rendering again.
  useLayoutEffect(() => {
    const section = sectionRef.current;
    const body = bodyRef.current;
    const content = contentRef.current;
    if (!open || section === null || body === null || content === null) return;
    const fade = () => {
      body.dataset.fade = String(body.scrollHeight - body.scrollTop - body.clientHeight > 1);
    };
    const measure = () => {
      section.style.setProperty("--section-header", `${headerRowRef.current?.offsetHeight ?? 0}px`);
      section.style.setProperty("--section-content", `${content.offsetHeight}px`);
      fade();
    };
    measure();
    body.addEventListener("scroll", fade, { passive: true });
    const observer = typeof ResizeObserver === "function" ? new ResizeObserver(measure) : null;
    observer?.observe(content);
    observer?.observe(body);
    return () => {
      body.removeEventListener("scroll", fade);
      observer?.disconnect();
    };
  }, [open]);

  // Opened during render (not in an effect, which `react-hooks/set-state-in-effect` forbids), so
  // the body is there when an owner reveals something inside it in the same commit.
  const [seenRevealRequest, setSeenRevealRequest] = useState(revealRequest);
  if (revealRequest !== seenRevealRequest) {
    setSeenRevealRequest(revealRequest);
    if (revealRequest !== undefined) setOpen(true);
  }
  const scrolledRevealRequest = useRef(revealRequest);
  useEffect(() => {
    if (revealRequest === undefined || revealRequest === scrolledRevealRequest.current) return;
    scrolledRevealRequest.current = revealRequest;
    persistOpen(storageKey, true);
    sectionRef.current?.scrollIntoView?.({ behavior: scrollBehavior(), block: "nearest" });
  }, [revealRequest, storageKey]);
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
    <section ref={sectionRef} className={styles.section} data-open={open} aria-label={title}>
      {/* WAI-ARIA APG accordion pattern: a heading wrapping the trigger button (so the section
          carries the heading role), plus roving-tabindex keyboard nav across sibling headers via
          `AccordionGroup`. Open state itself is independent per section by design (not mutual
          exclusion) — each section persists its own open/closed state under its own
          `storageKey`, so opening one is not meant to close its siblings. */}
      <div ref={headerRowRef} className={styles.headerRow} data-part="header">
        {createElement(
          `h${headingLevel}`,
          { className: styles.heading },
          <button
            ref={headerRef}
            type="button"
            // A nested section's title is a name, kept as written (#1012).
            className={headingLevel === 3 ? `${styles.header} ${styles.subHeader}` : styles.header}
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
        {actions !== undefined && <div className={styles.actions}>{actions}</div>}
      </div>
      {open && (
        <div ref={bodyRef} className={styles.body} data-part="body">
          <div ref={contentRef} className={styles.content} data-part="content">
            {children}
          </div>
        </div>
      )}
    </section>
  );
}
