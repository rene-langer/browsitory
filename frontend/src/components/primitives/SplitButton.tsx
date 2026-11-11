import { useId, useState } from "react";
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
  title,
  menuLabel,
  menuTitle,
  items,
  className,
  chevronClassName,
  describedBy,
  action,
  reason = null,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
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
}) {
  const [menu, setMenu] = useState<{ x: number; y: number; returnFocusTo: HTMLElement } | null>(null);
  const reasonId = useId();
  const blocked = reason !== null;
  const blockedProps = blocked
    ? { "aria-disabled": true as const, "aria-describedby": [describedBy, reasonId].filter(Boolean).join(" ") }
    : {};
  return (
    <span className={`${styles.group} ${reasonStyles.wrap}`}>
      <button
        type="button"
        className={className}
        disabled={disabled}
        title={blocked ? undefined : title}
        aria-describedby={describedBy}
        data-action={action}
        {...blockedProps}
        onClick={() => {
          if (!blocked) onClick();
        }}
      >
        {label}
      </button>
      <button
        type="button"
        className={chevronClassName ? `${styles.chevron} ${chevronClassName}` : styles.chevron}
        aria-label={menuLabel}
        aria-haspopup="menu"
        aria-expanded={menu !== null}
        disabled={disabled}
        title={blocked ? undefined : (menuTitle ?? title)}
        {...blockedProps}
        onClick={(event) => {
          if (blocked) return;
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
