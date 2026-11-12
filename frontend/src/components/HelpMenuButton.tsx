import { useState } from "react";
import { CircleHelp } from "lucide-react";
import { rectAnchor } from "../lib/menuKeys";
import { requestWorkspace } from "../lib/workspaceRequests";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";

/**
 * The header's Help menu (#647): the user guide, the keyboard shortcut sheet (while a repository
 * is open: the active workspace owns it), what's new and, in the desktop app, Check for updates
 * (#852).
 */
export function HelpMenuButton({
  className,
  shortcuts,
  onOpenUserGuide,
  onShowReleaseNotes,
  onCheckForUpdates,
}: {
  className: string;
  /** Whether a repository workspace is open to show the shortcut sheet. */
  shortcuts: boolean;
  onOpenUserGuide: () => void;
  onShowReleaseNotes: () => void;
  /** The palette's update check; absent where there is no updater (VSCode). */
  onCheckForUpdates?: () => void;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number; returnFocusTo: HTMLElement | undefined } | null>(null);
  const items: ContextMenuItem[] = [
    { label: "User guide", onSelect: onOpenUserGuide },
    ...(shortcuts
      ? [{ label: "Keyboard shortcuts", onSelect: () => requestWorkspace("shortcuts"), shortcut: "?", ariaKeyShortcuts: "?" }]
      : []),
    { label: "What’s new", onSelect: onShowReleaseNotes },
    ...(onCheckForUpdates !== undefined ? [{ label: "Check for updates", onSelect: onCheckForUpdates }] : []),
  ];
  return (
    <>
      <button
        type="button"
        className={className}
        aria-label="Help"
        title="Help"
        aria-haspopup="menu"
        aria-expanded={menu !== null}
        onClick={(event) => {
          const button = event.currentTarget;
          if (menu !== null) {
            button.focus({ preventScroll: true });
            setMenu(null);
            return;
          }
          // A pointer click need not focus the button (WebKit), so name it as where focus returns.
          setMenu({ ...rectAnchor(button), returnFocusTo: event.detail > 0 ? button : undefined });
        }}
      >
        <CircleHelp size={16} aria-hidden="true" />
      </button>
      {menu !== null && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          returnFocusTo={menu.returnFocusTo}
          items={items}
          onClose={() => setMenu(null)}
        />
      )}
    </>
  );
}
