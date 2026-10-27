import { useState } from "react";
import { ChevronDown } from "lucide-react";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";
import styles from "./SplitButton.module.css";

/**
 * A main action button with a chevron that opens a menu of related actions (SyncBar's Fetch,
 * the commit dock's Stash). The two buttons read as one control; the chevron's `aria-label` is
 * `menuLabel`. `disabled` disables both; `title` goes on the main button, `menuTitle` (default
 * `title`) on the chevron.
 */
export function SplitButton({
  label,
  onClick,
  disabled = false,
  title,
  menuLabel,
  menuTitle,
  items,
  chevronClassName,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  title?: string;
  menuLabel: string;
  menuTitle?: string;
  items: ContextMenuItem[];
  /** Extra class for the chevron, for callers whose own button styles need overriding. */
  chevronClassName?: string;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  return (
    <span className={styles.group}>
      <button type="button" disabled={disabled} title={title} onClick={() => onClick()}>
        {label}
      </button>
      <button
        type="button"
        className={chevronClassName ? `${styles.chevron} ${chevronClassName}` : styles.chevron}
        aria-label={menuLabel}
        aria-haspopup="menu"
        aria-expanded={menu !== null}
        disabled={disabled}
        title={menuTitle ?? title}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu({ x: rect.left, y: rect.bottom });
        }}
      >
        <ChevronDown size={12} aria-hidden="true" />
      </button>
      {menu !== null && (
        <ContextMenu x={menu.x} y={menu.y} items={items} onClose={() => setMenu(null)} />
      )}
    </span>
  );
}
