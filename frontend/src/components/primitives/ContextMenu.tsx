import { useEffect, useRef } from "react";
import styles from "./ContextMenu.module.css";

export interface ContextMenuItem {
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  destructive?: boolean;
  /** Rendered as the item button's `title` — e.g. explaining why it's disabled (issue #31/UX-003). */
  title?: string;
}

export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}) {
  const menuRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    function handleClickOutside(event: MouseEvent) {
      if (menuRef.current !== null && !menuRef.current.contains(event.target as Node)) onClose();
    }
    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [onClose]);

  return (
    <ul
      ref={menuRef}
      role="menu"
      className={styles.menu}
      style={{ position: "fixed", top: y, left: x }}
      onMouseLeave={onClose}
    >
      {items.map((item) => (
        <li key={item.label} role="none">
          <button
            type="button"
            role="menuitem"
            className={styles.item}
            disabled={item.disabled}
            title={item.title}
            data-destructive={item.destructive === true ? "true" : undefined}
            onClick={() => {
              if (item.disabled === true) return;
              item.onSelect();
              onClose();
            }}
          >
            {item.label}
          </button>
        </li>
      ))}
    </ul>
  );
}
