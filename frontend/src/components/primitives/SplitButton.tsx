import { useId, useState, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";
import { DisabledReasonTip } from "./ReasonButton";
import reasonStyles from "./ReasonButton.module.css";
import styles from "./SplitButton.module.css";

/**
 * A main action button with a chevron that opens a menu of related actions (SyncBar's Fetch,
 * the commit dock's Stash). The two buttons read as one control; the chevron's `aria-label` is
 * `menuLabel`. `disabled` disables both; `title` goes on the main button, `menuTitle` (default
 * `title`) on the chevron. A `reason` disables both too, as `aria-disabled` with the reason shown
 * on hover and focus and as their description (#664), instead of a `title`.
 */
export function SplitButton({
  label,
  onClick,
  disabled = false,
  menuDisabled = disabled,
  title,
  menuLabel,
  menuTitle,
  items,
  className,
  chevronClassName,
  describedBy,
  action,
  reason = null,
  menuReason = reason,
  badge,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  /** Disables the chevron; defaults to `disabled`. The commit box's menu stays open to amend
   * and co-authors while Commit waits for a message (#1003). */
  menuDisabled?: boolean;
  title?: string;
  menuLabel: string;
  menuTitle?: string;
  items: ContextMenuItem[];
  /** Class for the main button (the commit dock's primary style). */
  className?: string;
  /** Extra class for the chevron, for callers whose own button styles need overriding. */
  chevronClassName?: string;
  /** `aria-describedby` of the main button: the visible reason it is disabled. */
  describedBy?: string;
  /** `data-action` of the main button, a stable hook for the end-to-end tests. */
  action?: string;
  /** Why neither part can run now (#664); `null` leaves them to `disabled`. */
  reason?: string | null;
  /**
   * Why the menu can't open; defaults to `reason`. `null` keeps the menu open to items that can
   * still run while the main action is blocked (Stash's Apply patch…, #1006).
   */
  menuReason?: string | null;
  /** Shown after the label, hidden from assistive tech (Pull's "↓2", #1006). */
  badge?: ReactNode;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number; returnFocusTo: HTMLElement } | null>(null);
  const reasonId = useId();
  const blocked = reason !== null;
  const menuBlocked = menuReason !== null;
  const blockedProps = (on: boolean) =>
    on ? { "aria-disabled": true as const, "aria-describedby": [describedBy, reasonId].filter(Boolean).join(" ") } : {};
  return (
    <span className={`${styles.group} ${reasonStyles.wrap}`}>
      <button
        type="button"
        className={className}
        disabled={disabled}
        title={blocked ? undefined : title}
        aria-describedby={describedBy}
        data-action={action}
        {...blockedProps(blocked)}
        onClick={() => {
          if (!blocked) onClick();
        }}
      >
        {label}
        {badge}
      </button>
      <button
        type="button"
        className={chevronClassName ? `${styles.chevron} ${chevronClassName}` : styles.chevron}
        aria-label={menuLabel}
        aria-haspopup="menu"
        aria-expanded={menu !== null}
        disabled={menuDisabled}
        title={menuBlocked ? undefined : (menuTitle ?? title)}
        {...blockedProps(menuBlocked)}
        onClick={(event) => {
          if (menuBlocked) return;
          const chevron = event.currentTarget;
          if (menu !== null) {
            // A second click closes the menu; focus stays on the chevron (WebKit may not have
            // focused it on the click).
            chevron.focus({ preventScroll: true });
            setMenu(null);
            return;
          }
          const rect = chevron.getBoundingClientRect();
          setMenu({ x: rect.left, y: rect.bottom, returnFocusTo: chevron });
        }}
      >
        <ChevronDown size={12} aria-hidden="true" />
      </button>
      {blocked && <DisabledReasonTip id={reasonId} reason={reason} />}
      {menu !== null && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          returnFocusTo={menu.returnFocusTo}
          items={items}
          onClose={() => setMenu(null)}
        />
      )}
    </span>
  );
}
