import { useState, type KeyboardEvent } from "react";
import { MoreHorizontal, Package } from "lucide-react";
import type { SubmoduleInfo } from "../ipc/RepoClient";
import { isMenuKey, rectAnchor } from "../lib/menuKeys";
import { AccordionSection } from "./primitives/AccordionSection";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import { ListRow } from "./primitives/ListRow";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./SubmodulePanel.module.css";

// `returnFocusTo`: the row's "…" for a pointer (WebKit focuses neither the row nor the button);
// `undefined` for a key, so `ContextMenu` returns focus to what had it (as TagPanel).
type SubmoduleMenu = { path: string; x: number; y: number; returnFocusTo?: HTMLElement };

/** Why a submodule has no folder to open yet (#445); `null` when it is checked out. */
function notOpenableReason(submodule: SubmoduleInfo): string | null {
  if (submodule.workdir !== null) return null;
  return submodule.initialized
    ? "Not checked out: update it to check it out first."
    : "Not initialized: initialize and update it to check it out first.";
}

/**
 * Each submodule recorded in `.gitmodules`, with Initialize / Update buttons and a row menu
 * (the "…" button, right-click, or the Menu key / Shift+F10 on the row's buttons): Open in new
 * tab, which opens the checked-out submodule as a repository tab of its own, like a worktree
 * (#445), then Initialize (until it is) and Update.
 */
export function SubmodulePanel({
  submodules,
  onInit,
  onUpdate,
  onOpenInNewTab,
  operationDisabled,
  operationDisabledReason,
  revealRequest,
}: {
  submodules: SubmoduleInfo[];
  onInit: (path: string) => Promise<void>;
  onUpdate: (path: string, recursive: boolean) => Promise<void>;
  /** Opens a repository tab at the submodule's folder (`SubmoduleInfo.workdir`). */
  onOpenInNewTab: (workdir: string) => void;
  operationDisabled: boolean;
  // Human-readable reason `operationDisabled` is true, shown as a `title` on the menu items it
  // disables (issue #31/UX-003). `null` when nothing is blocking.
  operationDisabledReason: string | null;
  /** Expands and scrolls to the section (`useSidebarReveal`, #494). */
  revealRequest?: number;
}) {
  const [recursive, setRecursive] = useState(false);
  const [pendingPath, setPendingPath] = useState<string | null>(null);
  const [menu, setMenu] = useState<SubmoduleMenu | null>(null);

  // The menu acts on the submodule it was opened for; a refresh that removed it closes the menu.
  const menuSubmodule = menu === null ? undefined : submodules.find((submodule) => submodule.path === menu.path);
  if (menu !== null && menuSubmodule === undefined) setMenu(null);

  const runMutation = async (path: string, mutation: () => Promise<void>) => {
    setPendingPath(path);
    try {
      await mutation();
    } finally {
      setPendingPath(null);
    }
  };

  // The row has no click action of its own, so the Menu key and Shift+F10 open its menu from
  // its buttons.
  const openFromKey = (event: KeyboardEvent<HTMLElement>, path: string) => {
    if (!isMenuKey(event)) return;
    event.preventDefault();
    const row = event.currentTarget.closest("li");
    setMenu({ path, ...rectAnchor(row ?? event.currentTarget) });
  };

  const menuItems = (submodule: SubmoduleInfo): ContextMenuItem[] => {
    const rowDisabled = operationDisabled || pendingPath === submodule.path;
    const busyTitle = operationDisabled ? (operationDisabledReason ?? undefined) : undefined;
    const notOpenable = notOpenableReason(submodule);
    const workdir = submodule.workdir;
    const items: ContextMenuItem[] = [
      {
        label: "Open in new tab",
        onSelect: () => {
          if (workdir !== null) onOpenInNewTab(workdir);
        },
        disabled: notOpenable !== null || operationDisabled,
        title: notOpenable ?? busyTitle,
      },
    ];
    if (!submodule.initialized) {
      items.push({
        label: "Initialize",
        onSelect: () => void runMutation(submodule.path, () => onInit(submodule.path)),
        disabled: rowDisabled,
        title: busyTitle,
      });
    }
    items.push({
      label: "Update",
      onSelect: () => void runMutation(submodule.path, () => onUpdate(submodule.path, recursive)),
      disabled: rowDisabled,
      title: busyTitle,
    });
    return items;
  };

  return (
    <AccordionSection
      title="Submodules"
      storageKey="sidebar-submodules"
      icon={Package}
      count={submodules.length}
      revealRequest={revealRequest}
    >
      <label>
        <input
          type="checkbox"
          checked={recursive}
          disabled={operationDisabled}
          onChange={(event) => setRecursive(event.target.checked)}
        />
        Update recursively
      </label>
      {submodules.length === 0 ? (
        <p className={styles.empty}>No submodules. They show up here once the repo's .gitmodules records one.</p>
      ) : (
        <ul className={styles.list} aria-label="Submodules">
          {submodules.map((submodule) => {
            const rowDisabled = operationDisabled || pendingPath === submodule.path;

            return (
              <ListRow
                key={submodule.path}
                onOpenMenu={(anchor, trigger) =>
                  setMenu({
                    path: submodule.path,
                    ...anchor,
                    returnFocusTo: trigger?.querySelector<HTMLElement>('button[aria-haspopup="menu"]') ?? undefined,
                  })
                }
              >
                <Package size={14} aria-hidden="true" className={styles.rowIcon} />
                <strong>{submodule.path}</strong>
                <span>{submodule.url ?? "No URL configured"}</span>
                <span>{submodule.gitlinkId ?? "No recorded gitlink"}</span>
                <span>{submodule.initialized ? "Initialized" : "Not initialized"}</span>
                {submodule.headId !== null && <span>{submodule.headId}</span>}
                <Toolbar>
                  <button
                    type="button"
                    disabled={rowDisabled}
                    aria-keyshortcuts="Shift+F10 ContextMenu"
                    onKeyDown={(event) => openFromKey(event, submodule.path)}
                    onClick={() => void runMutation(submodule.path, () => onInit(submodule.path))}
                  >
                    Initialize {submodule.path}
                  </button>
                  <button
                    type="button"
                    disabled={rowDisabled}
                    aria-keyshortcuts="Shift+F10 ContextMenu"
                    onKeyDown={(event) => openFromKey(event, submodule.path)}
                    onClick={() => void runMutation(submodule.path, () => onUpdate(submodule.path, recursive))}
                  >
                    Update {submodule.path}
                  </button>
                </Toolbar>
                <button
                  type="button"
                  className={styles.iconButton}
                  aria-label={`Actions for ${submodule.path}`}
                  aria-haspopup="menu"
                  aria-expanded={menu?.path === submodule.path}
                  aria-keyshortcuts="Shift+F10 ContextMenu"
                  data-menu-key={submodule.path}
                  onKeyDown={(event) => openFromKey(event, submodule.path)}
                  onClick={(event) => {
                    const trigger = event.currentTarget;
                    if (menu?.path === submodule.path) {
                      // A second click closes it; focus stays on the button.
                      trigger.focus({ preventScroll: true });
                      setMenu(null);
                    } else {
                      setMenu({ path: submodule.path, ...rectAnchor(trigger), returnFocusTo: trigger });
                    }
                  }}
                >
                  <MoreHorizontal size={14} aria-hidden="true" />
                </button>
              </ListRow>
            );
          })}
        </ul>
      )}
      {menu !== null && menuSubmodule !== undefined && (
        <ContextMenu
          key={menuSubmodule.path}
          x={menu.x}
          y={menu.y}
          returnFocusTo={menu.returnFocusTo}
          isTrigger={(button) => button.getAttribute("data-menu-key") === menuSubmodule.path}
          onClose={() => setMenu(null)}
          items={menuItems(menuSubmodule)}
        />
      )}
    </AccordionSection>
  );
}
