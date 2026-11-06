import { useEffect, useRef, useState } from "react";
import { Archive, MoreHorizontal } from "lucide-react";
import { rectAnchor } from "../lib/menuKeys";
import type { StashEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { AccordionSection } from "./primitives/AccordionSection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import { ListRow } from "./primitives/ListRow";
import styles from "./StashPanel.module.css";

// `returnFocusTo`: the "…" button or row it was opened from by the pointer (WebKit focuses
// neither); `undefined` for a key, so `ContextMenu` returns focus to what had it (as BranchTree).
type StashMenu = { stash: StashEntry; x: number; y: number; returnFocusTo?: HTMLElement };

/**
 * The Stashes sidebar section. Each row shows the stash message across the full width, with
 * Apply, Pop and Drop… in a menu (the "…" button, right-click, the Menu key or Shift+F10), the
 * same pattern as the Branches rows: three inline buttons left the message a few characters
 * wide in a 260px sidebar and wrapped onto several lines (#339).
 */
export function StashPanel({
  stashes,
  onSelectRow,
  onApplyStash,
  onPopStash,
  onDropStash,
  restoreIndex,
  onSetRestoreIndex,
  applyDisabledReason,
  operationDisabled,
  operationDisabledReason,
}: {
  stashes: StashEntry[];
  onSelectRow: (row: SelectedRow) => void;
  onApplyStash: (index: number) => void;
  onPopStash: (index: number) => void;
  onDropStash: (index: number) => void;
  // "Restore staged changes" for Apply and Pop (shared with the stash view).
  restoreIndex: boolean;
  onSetRestoreIndex: (on: boolean) => void;
  // Why Apply/Pop can't run right now (staged changes, conflicts, a running operation); `null`
  // when they can. Drop only follows `operationDisabled`.
  applyDisabledReason: string | null;
  operationDisabled: boolean;
  // Human-readable reason `operationDisabled` is true, shown as a `title` on the buttons it
  // disables so they don't just go inert with no explanation (issue #31/UX-003). `null` when
  // nothing is blocking.
  operationDisabledReason: string | null;
}) {
  const [pendingDrop, setPendingDrop] = useState<StashEntry | null>(null);
  const [menu, setMenu] = useState<StashMenu | null>(null);
  // The menu and the drop confirmation act on the stash they were opened for, looked up again by
  // commit id: a refresh while they are open can move it to another index or remove it (#339).
  const menuStash = menu === null ? undefined : stashes.find((stash) => stash.commitId === menu.stash.commitId);
  const dropStash = pendingDrop === null ? undefined : stashes.find((stash) => stash.commitId === pendingDrop.commitId);
  // Gone from the list: close them (state adjusted while rendering, as React recommends over an
  // effect), and remember where the stash was so focus does not drop to the page (below).
  const [refocus, setRefocus] = useState<{ index: number } | null>(null);
  if (menu !== null && menuStash === undefined) {
    setMenu(null);
    setRefocus({ index: menu.stash.index });
  }
  if (pendingDrop !== null && dropStash === undefined) {
    setPendingDrop(null);
    setRefocus({ index: pendingDrop.index });
  }
  // The list, or the empty-state text when there is none.
  const bodyRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (refocus === null) return;
    const active = document.activeElement;
    if (active !== null && active !== document.body && active.isConnected) return;
    // Focus went with the closed menu or dialog: move it to the "…" of the stash now in that
    // place (or the last one), else to the section's header.
    const buttons = bodyRef.current?.querySelectorAll<HTMLElement>('button[aria-haspopup="menu"]') ?? [];
    const next = buttons.length > 0 ? buttons[Math.min(refocus.index, buttons.length - 1)] : undefined;
    const header = bodyRef.current?.closest("section")?.querySelector<HTMLElement>("button");
    (next ?? header)?.focus({ preventScroll: true });
  }, [refocus]);
  const applyDisabled = operationDisabled || applyDisabledReason !== null;
  const applyTitle = applyDisabledReason ?? (operationDisabled ? (operationDisabledReason ?? undefined) : undefined);
  const dropTitle = operationDisabled ? (operationDisabledReason ?? undefined) : undefined;

  const menuItems = (stash: StashEntry): ContextMenuItem[] => [
    { label: "Apply", disabled: applyDisabled, title: applyTitle, onSelect: () => onApplyStash(stash.index) },
    { label: "Pop", disabled: applyDisabled, title: applyTitle, onSelect: () => onPopStash(stash.index) },
    {
      label: "Drop…",
      disabled: operationDisabled,
      title: dropTitle,
      destructive: true,
      onSelect: () => setPendingDrop(stash),
    },
  ];

  return (
    <AccordionSection title="Stashes" storageKey="sidebar-stashes" icon={Archive} count={stashes.length}>
      {stashes.length === 0 ? (
        <p
          ref={(element) => {
            bodyRef.current = element;
          }}
          className={styles.empty}
        >
          No stashes. Set changes aside from the commit box to see them here.
        </p>
      ) : (
        <>
          <label className={styles.restoreIndex}>
            <input
              type="checkbox"
              checked={restoreIndex}
              onChange={(event) => onSetRestoreIndex(event.target.checked)}
            />
            Restore staged changes
          </label>
          <ul
            ref={(element) => {
              bodyRef.current = element;
            }}
            className={styles.list}
          >
            {stashes.map((stash) => (
              <ListRow
                key={stash.commitId}
                onClick={() => onSelectRow({ commitId: stash.commitId })}
                onOpenMenu={(anchor, trigger) => setMenu({ stash, ...anchor, returnFocusTo: trigger })}
              >
                <Archive size={14} aria-hidden="true" className={styles.rowIcon} />
                <span className={styles.message} title={stash.message}>
                  {stash.message}
                </span>
                <button
                  type="button"
                  className={styles.iconButton}
                  aria-label={`Actions for ${stash.message}`}
                  aria-haspopup="menu"
                  aria-expanded={menu?.stash.commitId === stash.commitId}
                  onClick={(event) => {
                    event.stopPropagation();
                    const trigger = event.currentTarget;
                    setMenu((open) =>
                      open?.stash.commitId === stash.commitId
                        ? null
                        : { stash, ...rectAnchor(trigger), returnFocusTo: trigger },
                    );
                  }}
                >
                  <MoreHorizontal size={14} aria-hidden="true" />
                </button>
              </ListRow>
            ))}
          </ul>
        </>
      )}
      {menu !== null && menuStash !== undefined && (
        <ContextMenu
          key={menuStash.commitId}
          x={menu.x}
          y={menu.y}
          returnFocusTo={menu.returnFocusTo}
          onClose={() => setMenu(null)}
          items={menuItems(menuStash)}
        />
      )}
      {dropStash !== undefined && (
        <ConfirmDialog
          ariaLabel={`Drop stash ${dropStash.message}`}
          message={<p>Drop "{dropStash.message}"? This cannot be undone.</p>}
          confirmLabel="Drop stash"
          confirmDisabled={operationDisabled}
          confirmTitle={dropTitle}
          onConfirm={() => {
            onDropStash(dropStash.index);
            setPendingDrop(null);
          }}
          onCancel={() => setPendingDrop(null)}
        />
      )}
    </AccordionSection>
  );
}
