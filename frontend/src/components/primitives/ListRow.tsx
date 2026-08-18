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
 *   stops, and the container is the single tab stop that moves the selection.
 */
export function ListRow({
  selected,
  onClick,
  onContextMenu,
  className,
  children,
}: {
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

  const handleKeyDown = (event: KeyboardEvent<HTMLLIElement>) => {
    if (onClick === undefined) return;
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onClick();
    }
  };

  return (
    <li
      className={[styles.row, interactive ? styles.interactive : null, className]
        .filter((part) => part !== null && part !== undefined && part !== "")
        .join(" ")}
      role={standalone ? "button" : undefined}
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
