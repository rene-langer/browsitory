import { useState } from "react";
import { Archive, MoreHorizontal } from "lucide-react";
import { rectAnchor } from "../lib/menuKeys";
import type { StashEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { AccordionSection } from "./primitives/AccordionSection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { DropStashMessage } from "./DropStashMessage";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import { ListRow } from "./primitives/ListRow";
import styles from "./StashPanel.module.css";
import { useRefocusRemovedRow } from "./useRefocusRemovedRow";
import { useVirtualRows } from "./useVirtualRows";
import { menuItem } from "../lib/actions/action";
import { stashActions, stashDetails, stashTitle, type StashBlockers } from "../lib/actions/stashActions";
import { formatAbsoluteDate } from "../lib/formatDate";
import { useDateFormat } from "../state/useDateFormat";

// `returnFocusTo`: the "…" button or row it was opened from by the pointer (WebKit focuses
// neither); `undefined` for a key, so `ContextMenu` returns focus to what had it (as BranchTree).
type StashMenu = { stash: StashEntry; x: number; y: number; returnFocusTo?: HTMLElement };

/**
 * The Stashes sidebar section. Each row shows the stash's name across the full width with "On
 * main · 2 days ago · 3 files" below it (#657), and Apply, Pop and Drop… in a menu (the "…"
 * button, right-click, the Menu key or Shift+F10), the same pattern as the Branches rows: three
 * inline buttons left the message a few characters wide in a 260px sidebar and wrapped onto
 * several lines (#339). Apply and Pop restore staged changes as staged; a stash that recorded
 * staged changes also offers "Apply as unstaged" and "Pop as unstaged" (#657, which replaced a
 * list-wide "Restore staged changes" checkbox).
 */
export function StashPanel({
  stashes,
  onSelectRow,
  onApplyStash,
  onPopStash,
  onDropStash,
  blockers,
  revealRequest,
}: {
  stashes: StashEntry[];
  onSelectRow: (row: SelectedRow) => void;
  // `restoreIndex`: false for "Apply/Pop as unstaged".
  onApplyStash: (index: number, restoreIndex: boolean) => void;
  onPopStash: (index: number, restoreIndex: boolean) => void;
  // A promise settles when the drop has finished, failed or been refused.
  onDropStash: (index: number) => void | Promise<unknown>;
  // Why Apply/Pop and Drop can't run now (`stashBlockers`, shared with the stash view and the
  // palette, #509), shown as the disabled items' titles (issue #31/UX-003). Drop is blocked only
  // by an action in flight or a transfer: dropping a stash while resolving a paused merge, rebase
  // or sequence step is fine (#521).
  blockers: StashBlockers;
  /** Expands and scrolls to the section (`useSidebarReveal`, #494). */
  revealRequest?: number;
}) {
  const dateFormat = useDateFormat();
  const [pendingDrop, setPendingDrop] = useState<StashEntry | null>(null);
  const [menu, setMenu] = useState<StashMenu | null>(null);
  // The menu and the drop confirmation act on the stash they were opened for, looked up again by
  // commit id: a refresh while they are open can move it to another index or remove it (#339).
  const menuStash = menu === null ? undefined : stashes.find((stash) => stash.commitId === menu.stash.commitId);
  const dropStash = pendingDrop === null ? undefined : stashes.find((stash) => stash.commitId === pendingDrop.commitId);

  // A stash this panel acts on that leaves the list (its menu or drop confirmation is open, or its
  // drop still runs and the focus went back to its "…"): the focus goes to the stash now in that
  // place (`useRefocusRemovedRow`, as in the other sidebar lists).
  const [droppedCommitId, setDroppedCommitId] = useState<string | null>(null);
  if (droppedCommitId !== null && !stashes.some((stash) => stash.commitId === droppedCommitId)) {
    setDroppedCommitId(null);
  }
  if (menu !== null && menuStash === undefined) setMenu(null);
  if (pendingDrop !== null && dropStash === undefined) setPendingDrop(null);
  // Only the rows in view render (#775), as in the commit graph (#685).
  const [listElement, setListElement] = useState<HTMLUListElement | null>(null);
  const virtual = useVirtualRows(listElement, stashes.length);
  useRefocusRemovedRow({
    list: listElement,
    watching: [menu?.stash.commitId, pendingDrop?.commitId, droppedCommitId],
  });
  const dropDisabledReason = blockers.drop;
  const dropDisabled = dropDisabledReason !== null;

  // A new menu or Drop dialog ends watching an earlier drop (its stash may never go).
  const openMenu = (next: StashMenu) => {
    setDroppedCommitId(null);
    setMenu(next);
  };

  const menuItems = (stash: StashEntry): ContextMenuItem[] => {
    const actions = stashActions(blockers, stash);
    return [
      menuItem(actions.apply, () => onApplyStash(stash.index, true)),
      ...(actions.applyUnstaged === null ? [] : [menuItem(actions.applyUnstaged, () => onApplyStash(stash.index, false))]),
      menuItem(actions.pop, () => onPopStash(stash.index, true)),
      ...(actions.popUnstaged === null ? [] : [menuItem(actions.popUnstaged, () => onPopStash(stash.index, false))]),
      {
        ...menuItem(actions.drop, () => {
          setDroppedCommitId(null);
          setPendingDrop(stash);
        }),
        destructive: true,
        // Its own group (#645), apart from Apply and Pop.
        group: "drop",
      },
    ];
  };

  return (
    <AccordionSection
      title="Stashes"
      storageKey="sidebar-stashes"
      icon={Archive}
      count={stashes.length}
      revealRequest={revealRequest}
    >
      {stashes.length === 0 ? (
        <p className={styles.empty}>No stashes. Use Stash in the repository toolbar to set your changes aside.</p>
      ) : (
        <>
          <ul
            ref={setListElement}
            className={styles.list}
            style={virtual.windowed ? { paddingTop: virtual.padTop, paddingBottom: virtual.padBottom } : undefined}
          >
            {stashes.slice(virtual.start, virtual.end).map((stash, offset) => {
              const title = stashTitle(stash);
              const details = stashDetails(stash);
              return (
                <ListRow
                  key={stash.commitId}
                  rowIndex={virtual.start + offset}
                  onClick={() => onSelectRow({ commitId: stash.commitId })}
                  onOpenMenu={(anchor, trigger) => openMenu({ stash, ...anchor, returnFocusTo: trigger })}
                >
                  <Archive size={14} aria-hidden="true" className={styles.rowIcon} />
                  <span className={styles.text} title={`${stash.message}\n${formatAbsoluteDate(stash.timestamp, dateFormat)}`}>
                    <span className={styles.message}>{title}</span>
                    <span className={styles.details}>{details}</span>
                  </span>
                  <button
                    type="button"
                    className={styles.iconButton}
                    aria-label={`Actions for ${title}`}
                    aria-haspopup="menu"
                    aria-expanded={menu?.stash.commitId === stash.commitId}
                    data-menu-key={stash.commitId}
                    onClick={(event) => {
                      event.stopPropagation();
                      const trigger = event.currentTarget;
                      if (menu?.stash.commitId === stash.commitId) {
                        // A second click closes it; focus stays on the button (WebKit may not have
                        // focused it on the click).
                        trigger.focus({ preventScroll: true });
                        setMenu(null);
                      } else {
                        openMenu({ stash, ...rectAnchor(trigger), returnFocusTo: trigger });
                      }
                    }}
                  >
                    <MoreHorizontal size={14} aria-hidden="true" />
                  </button>
                </ListRow>
              );
            })}
          </ul>
        </>
      )}
      {menu !== null && menuStash !== undefined && (
        <ContextMenu
          key={menuStash.commitId}
          x={menu.x}
          y={menu.y}
          returnFocusTo={menu.returnFocusTo}
          // Opened from the row by a key or a right-click, the row's "…" is still its own button.
          isTrigger={(button) => button.getAttribute("data-menu-key") === menuStash.commitId}
          onClose={() => setMenu(null)}
          items={menuItems(menuStash)}
        />
      )}
      {dropStash !== undefined && (
        <ConfirmDialog
          title={`Drop stash ${stashTitle(dropStash)}`}
          message={<DropStashMessage />}
          confirmLabel="Drop stash"
          tone="danger"
          // #521: the repository can become busy while the confirmation is open.
          confirmReason={dropDisabledReason}
          onConfirm={() => {
            if (dropDisabled) return;
            const { commitId } = dropStash;
            setDroppedCommitId(commitId);
            setPendingDrop(null);
            // Once the drop has finished (its refresh included), failed or been refused, a later
            // removal of this stash is no longer this panel's doing: stop watching it.
            void Promise.resolve(onDropStash(dropStash.index))
              .catch(() => undefined)
              .finally(() => setDroppedCommitId((current) => (current === commitId ? null : current)));
          }}
          onCancel={() => setPendingDrop(null)}
        />
      )}
    </AccordionSection>
  );
}
