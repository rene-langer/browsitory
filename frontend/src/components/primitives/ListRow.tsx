import type { KeyboardEvent, MouseEvent, ReactNode } from "react";
import styles from "./ListRow.module.css";

/**
 * A row inside a list. Which interaction semantics it renders is decided entirely by the props
 * it is given, so a consumer can never accidentally get an affordance it doesn't implement:
 *
 * - **No `onClick`** — a plain, non-interactive `<li>`: no pointer cursor, no hover tint, no
 *   role. Used by lists whose rows are just containers for their own controls (RebasePlanner's
 *   plan rows, ReflogPanel's entries), which previously had to pass a no-op `onClick` and then
 *   undo the clickable styling in their own CSS module.
 * - **`onClick` without `selected`** — a standalone activatable row, i.e. a real control:
 *   `role="button"`, `tabIndex={0}`, an Enter/Space key handler and a visible focus ring, so it
 *   behaves and is announced exactly like the `<button>` it replaces (RepoPicker's recent
 *   repositories).
 * - **`onClick` with `selected`** — a row in a list whose *container* owns selection and
 *   keyboard navigation (CommitGraph's `<ul tabIndex={0}>` with arrow/j/k keys). The row stays
 *   out of the tab order on purpose: a thousand-commit history must not become a thousand tab
 *   stops, and the container is the single tab stop that moves the selection. Gets
 *   `role="option"` so `aria-selected` is valid ARIA (it's otherwise meaningless on a plain
 *   `listitem`) — the container must carry a matching `role="listbox"` for this to form a
 *   complete pattern.
 *
 * ## Interactive controls inside a `role="option"` row
 *
 * ARIA's listbox/option pattern expects an option's children to be content (text, graphics)
 * that contributes to the option's accessible name — not independent widgets. Nested `<button>`s
 * stay clickable and Tab-reachable, but assistive tech arrowing through the listbox does not
 * reliably announce or reach them. `CommitGraph` never hit this (its rows are text and graphics
 * only); `DiffPane`'s file rows, which carry Blame/Stage/Unstage buttons, are the first.
 *
 * The decision there — and the one to copy — was **not** to change this primitive, and not to
 * lift the controls out of the row (a sibling of an `<li>` inside a `<ul>` has to be another
 * `<li>`, which breaks the list semantics this pattern depends on). Instead the row's controls
 * stay as the mouse/direct-Tab affordance they already are, and the *container* gains a key
 * binding for the same action on the selected row — `s` to stage/unstage in `DiffPane`,
 * alongside the `j`/`k`/arrow navigation the container already owns (advertised with
 * `aria-keyshortcuts`). The container is the single tab stop, so that keeps the whole flow
 * keyboard-reachable without inventing a second focus model inside each row.
 */
export function ListRow({
  id,
  selected,
  onClick,
  onContextMenu,
  className,
  children,
}: {
  id?: string;
  selected?: boolean;
  onClick?: () => void;
  onContextMenu?: (event: MouseEvent) => void;
  className?: string;
  children: ReactNode;
}) {
  const interactive = onClick !== undefined;
  // `selected` present means the surrounding list tracks a selection and therefore already
  // handles keyboard navigation itself — see the doc comment above.
  const standalone = interactive && selected === undefined;
  const containerOwnsSelection = interactive && selected !== undefined;

  const handleKeyDown = (event: KeyboardEvent<HTMLLIElement>) => {
    if (onClick === undefined) return;
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onClick();
    }
  };

  return (
    <li
      id={id}
      className={[styles.row, interactive ? styles.interactive : null, className]
        .filter((part) => part !== null && part !== undefined && part !== "")
        .join(" ")}
      role={standalone ? "button" : containerOwnsSelection ? "option" : undefined}
      tabIndex={standalone ? 0 : undefined}
      aria-selected={selected}
      onClick={onClick}
      onKeyDown={standalone ? handleKeyDown : undefined}
      onContextMenu={onContextMenu}
    >
      {children}
    </li>
  );
}
