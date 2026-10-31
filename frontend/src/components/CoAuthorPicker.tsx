import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import type { SignatureInput } from "../ipc/RepoClient";
import { placeAnchored, type Placement } from "../lib/anchoredPosition";
import { formatPerson as label, parsePerson as typedPerson } from "../lib/people";
import styles from "./CommitBox.module.css";

/**
 * "Add co-author": a small popover listing the repository's recent authors (loaded each time it
 * opens), filtered as you type. A `Name <email>` typed into the filter can be added too.
 *
 * The popover is `position: fixed` and placed with `placeAnchored` (#141): below the button when
 * it fits, otherwise above, shifted to stay inside the window. It re-measures when it opens, when
 * its content changes (the authors load asynchronously), and on window resize and any scroll.
 */
export function CoAuthorPicker({
  load,
  exclude,
  onPick,
}: {
  load: () => Promise<SignatureInput[]>;
  /** Already chosen (matched by email, case-insensitively). */
  exclude: SignatureInput[];
  onPick: (person: SignatureInput) => void;
}) {
  const [open, setOpen] = useState(false);
  const [authors, setAuthors] = useState<SignatureInput[] | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [filter, setFilter] = useState("");
  const [active, setActive] = useState(0);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const buttonRef = useRef<HTMLButtonElement | null>(null);
  const popoverRef = useRef<HTMLDivElement | null>(null);
  const [placement, setPlacement] = useState<Placement | null>(null);
  const listId = useId();

  const place = useCallback(() => {
    const button = buttonRef.current;
    const popover = popoverRef.current;
    if (button === null || popover === null) return;
    // Measured uncapped: once placed, `maxHeight` shrinks the list, and the capped height would
    // always "fit" the side it was capped to. Restored before paint.
    const cap = popover.style.maxHeight;
    popover.style.maxHeight = "none";
    const size = { width: popover.offsetWidth, height: popover.scrollHeight };
    popover.style.maxHeight = cap;
    const next = placeAnchored(button.getBoundingClientRect(), size, {
      width: window.innerWidth,
      height: window.innerHeight,
    });
    setPlacement((current) =>
      current !== null &&
      current.top === next.top &&
      current.left === next.left &&
      current.maxHeight === next.maxHeight &&
      current.side === next.side
        ? current
        : next,
    );
  }, []);

  useEffect(() => {
    if (!open) return;
    // Capture: the diff pane (not the window) is what scrolls under the commit box.
    const options = { capture: true, passive: true } as const;
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, options);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, options);
    };
  }, [open, place]);

  useEffect(() => {
    if (!open) return;
    let ignore = false;
    Promise.resolve()
      .then(load)
      .then(
        (next) => {
          if (!ignore) setAuthors(next);
        },
        () => {
          if (!ignore) setLoadError(true);
        },
      );
    const onPointerDown = (event: MouseEvent) => {
      if (rootRef.current !== null && !rootRef.current.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onPointerDown);
    return () => {
      ignore = true;
      document.removeEventListener("mousedown", onPointerDown);
    };
  }, [open, load]);

  const close = () => {
    setOpen(false);
    buttonRef.current?.focus();
  };
  const excluded = new Set(exclude.map((p) => p.email.toLowerCase()));
  const needle = filter.trim().toLowerCase();
  const matches = (authors ?? []).filter(
    (p) => !excluded.has(p.email.toLowerCase()) && (needle === "" || label(p).toLowerCase().includes(needle)),
  );
  const typed = typedPerson(filter);
  const options =
    typed !== null && !excluded.has(typed.email.toLowerCase()) && !matches.some((p) => p.email.toLowerCase() === typed.email.toLowerCase())
      ? [...matches, typed]
      : matches;
  const activeIndex = Math.min(active, Math.max(options.length - 1, 0));

  // Measured before paint, so the popover never flashes at its unplaced position.
  useLayoutEffect(() => {
    if (open) place();
  }, [open, place, authors, options.length, loadError]);
  const pick = (person: SignatureInput) => {
    onPick(person);
    close();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (options.length === 0) return;
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((activeIndex + step + options.length) % options.length);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const person = options[activeIndex];
      if (person !== undefined) pick(person);
    }
  };

  return (
    <div className={styles.picker} ref={rootRef}>
      <button
        type="button"
        ref={buttonRef}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => {
          if (open) {
            setOpen(false);
            return;
          }
          setFilter("");
          setActive(0);
          setAuthors(null);
          setLoadError(false);
          setPlacement(null);
          setOpen(true);
        }}
      >
        Add co-author
      </button>
      {open && (
        <div
          ref={popoverRef}
          className={styles.popover}
          data-side={placement?.side}
          style={
            placement === null
              ? { visibility: "hidden" }
              : { top: placement.top, left: placement.left, maxHeight: placement.maxHeight }
          }
        >
          <input
            type="text"
            aria-label="Filter authors"
            placeholder="Filter, or type Name <email>"
            spellCheck={false}
            autoFocus
            role="combobox"
            aria-expanded="true"
            aria-controls={listId}
            aria-activedescendant={options.length > 0 ? `${listId}-${activeIndex}` : undefined}
            value={filter}
            onChange={(event) => {
              setFilter(event.target.value);
              setActive(0);
            }}
            onKeyDown={onKeyDown}
          />
          {loadError ? (
            <p className={styles.pickerNote}>Couldn't load recent authors.</p>
          ) : authors === null ? (
            <p role="status" className={styles.pickerNote}>
              Loading…
            </p>
          ) : null}
          <ul id={listId} role="listbox" aria-label="Recent authors" className={styles.options}>
            {options.map((person, index) => (
              <li
                key={person.email.toLowerCase()}
                id={`${listId}-${index}`}
                role="option"
                aria-selected={index === activeIndex}
                className={styles.option}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => pick(person)}
              >
                {label(person)}
              </li>
            ))}
          </ul>
          {authors !== null && options.length === 0 && (
            <p className={styles.pickerNote}>{needle === "" ? "No other recent authors." : "No matching authors."}</p>
          )}
        </div>
      )}
    </div>
  );
}
