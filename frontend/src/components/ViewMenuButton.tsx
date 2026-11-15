import { useState } from "react";
import { ChevronDown } from "lucide-react";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";

/**
 * "View ▾" (#1007): the rarely changed view toggles of a pane, as checkable items with text
 * labels, instead of a row of icon buttons whose meaning was only in a tooltip. `label` names it
 * for assistive tech ("Diff view", "Graph view"); a second click closes the menu.
 */
export function ViewMenuButton({
  label,
  title,
  items,
  className,
}: {
  label: string;
  title: string;
  items: ContextMenuItem[];
  className?: string;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number; returnFocusTo: HTMLElement } | null>(null);
  return (
    <>
      <button
        type="button"
        className={className}
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={menu !== null}
        title={title}
        onClick={(event) => {
          const button = event.currentTarget;
          if (menu !== null) {
            button.focus({ preventScroll: true });
            setMenu(null);
            return;
          }
          const rect = button.getBoundingClientRect();
          setMenu({ x: rect.left, y: rect.bottom, returnFocusTo: button });
        }}
      >
        View
        <ChevronDown size={12} aria-hidden="true" />
      </button>
      {menu !== null && (
        <ContextMenu x={menu.x} y={menu.y} returnFocusTo={menu.returnFocusTo} items={items} onClose={() => setMenu(null)} />
      )}
    </>
  );
}
