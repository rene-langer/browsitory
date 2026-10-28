import type { KeyboardEvent, MouseEvent, ReactNode } from "react";
import { isMenuKey, menuAnchor, rectAnchor } from "../../lib/menuKeys";
import styles from "./ListRow.module.css";

type MenuAnchor = { x: number; y: number };

// `onContextMenu` (the raw event) and `onOpenMenu` (an anchor, keyboard included) are exclusive.
type MenuProps =
  | { onContextMenu?: (event: MouseEvent) => void; onOpenMenu?: never }
  | { onOpenMenu?: (anchor: MenuAnchor) => void; onContextMenu?: never };

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
 *
 * ## Context menus: `onOpenMenu`
 *
 * A row with a context menu passes `onOpenMenu` (instead of `onContextMenu`) and gets an anchor
 * to open it at: the pointer for a right-click, the row's bottom-left corner for a keyboard-
 * generated `contextmenu` (see `menuAnchor`). A standalone row also opens it from the Menu key
 * or Shift+F10, pressed on the row or on one of its nested buttons (not inside a text field,
 * e.g. an inline rename input), and advertises that with `aria-keyshortcuts`. Container-owned
 * rows only get the right-click: their container owns the keys (as `CommitGraph` does).
 */
export function ListRow({
  id,
  selected,
  onClick,
  onDoubleClick,
  onContextMenu,
  onOpenMenu,
  onMouseEnter,
  onMouseLeave,
  className,
  "data-commit-id": dataCommitId,
  children,
}: {
  id?: string;
  selected?: boolean;
  onClick?: (event?: MouseEvent) => void;
  // Standalone rows only (see below) — a row whose container owns selection has no use for a
  // second, "activate" action distinct from `onClick`.
  onDoubleClick?: (event?: MouseEvent) => void;
  onMouseEnter?: () => void;
  onMouseLeave?: () => void;
  className?: string;
  // Lets the command palette find a commit row to open its context menu.
  "data-commit-id"?: string;
  children: ReactNode;
} & MenuProps) {
  const interactive = onClick !== undefined;
  // `selected` present means the surrounding list tracks a selection and therefore already
  // handles keyboard navigation itself — see the doc comment above.
  const standalone = interactive && selected === undefined;
  const containerOwnsSelection = interactive && selected !== undefined;

  const handleKeyDown = (event: KeyboardEvent<HTMLLIElement>) => {
    if (onOpenMenu !== undefined && isMenuKey(event)) {
      // The row's nested buttons (swatch, "…") open the row's menu too; text fields keep the key.
      if ((event.target as HTMLElement).closest("input, textarea, select") !== null) return;
      event.preventDefault();
      onOpenMenu(rectAnchor(event.currentTarget));
      return;
    }
    if (onClick === undefined) return;
    // Keys bubbling from nested controls (swatch toggle, "..." button) belong to those controls.
    if (event.target !== event.currentTarget) return;
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      // Mirrors mouse behavior for a row with a distinct double-click "activate" action (e.g. a
      // tree row where a single click only selects): Enter activates rather than merely
      // selecting, since a keyboard user has no way to "double-press" Enter.
      if (onDoubleClick !== undefined) onDoubleClick();
      else onClick();
    }
  };

  return (
    <li
      id={id}
      data-commit-id={dataCommitId}
      className={[styles.row, interactive ? styles.interactive : null, className]
        .filter((part) => part !== null && part !== undefined && part !== "")
        .join(" ")}
      role={standalone ? "button" : containerOwnsSelection ? "option" : undefined}
      tabIndex={standalone ? 0 : undefined}
      aria-selected={selected}
      aria-haspopup={onOpenMenu !== undefined ? "menu" : undefined}
      aria-keyshortcuts={onOpenMenu !== undefined && standalone ? "Shift+F10 ContextMenu" : undefined}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      onKeyDown={standalone ? handleKeyDown : undefined}
      onContextMenu={
        onOpenMenu !== undefined
          ? (event) => {
              event.preventDefault();
              onOpenMenu(menuAnchor(event, event.currentTarget));
            }
          : onContextMenu
      }
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
    >
      {children}
    </li>
  );
}
