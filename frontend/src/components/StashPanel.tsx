import { useEffect, useRef, useState } from "react";
import { Archive, MoreHorizontal } from "lucide-react";
import { rectAnchor } from "../lib/menuKeys";
import type { StashEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { AccordionSection } from "./primitives/AccordionSection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import formStyles from "./primitives/FormDialog.module.css";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import { ListRow } from "./primitives/ListRow";
import styles from "./StashPanel.module.css";
import { menuItem } from "../lib/actions/action";
import { stashActions, type StashBlockers } from "../lib/actions/stashActions";

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
  blockers,
}: {
  stashes: StashEntry[];
  onSelectRow: (row: SelectedRow) => void;
  onApplyStash: (index: number) => void;
  onPopStash: (index: number) => void;
  // A promise settles when the drop has finished, failed or been refused.
  onDropStash: (index: number) => void | Promise<unknown>;
  // "Restore staged changes" for Apply and Pop (shared with the stash view).
  restoreIndex: boolean;
  onSetRestoreIndex: (on: boolean) => void;
  // Why Apply/Pop and Drop can't run now (`stashBlockers`, shared with the stash view and the
  // palette, #509), shown as the disabled items' titles (issue #31/UX-003). Drop is blocked only
  // by an action in flight or a transfer: dropping a stash while resolving a paused merge, rebase
  // or sequence step is fine (#521).
  blockers: StashBlockers;
}) {
  const [pendingDrop, setPendingDrop] = useState<StashEntry | null>(null);
  const [menu, setMenu] = useState<StashMenu | null>(null);
  // The menu and the drop confirmation act on the stash they were opened for, looked up again by
  // commit id: a refresh while they are open can move it to another index or remove it (#339).
  const menuStash = menu === null ? undefined : stashes.find((stash) => stash.commitId === menu.stash.commitId);
  const dropStash = pendingDrop === null ? undefined : stashes.find((stash) => stash.commitId === pendingDrop.commitId);

  // When a stash this panel had focus on leaves the list (its menu or drop confirmation was open,
  // or its drop is still running and focus went back to its "…"), focus would drop to the page.
  // Remember its place in the last list that held it, and move focus there after the commit.
  // State is adjusted while rendering, as React recommends over an effect.
  const [lastStashes, setLastStashes] = useState(stashes);
  const [droppedCommitId, setDroppedCommitId] = useState<string | null>(null);
  const [refocus, setRefocus] = useState<{ position: number } | null>(null);
  if (stashes !== lastStashes) {
    setLastStashes(stashes);
    const watched = [menu?.stash.commitId, pendingDrop?.commitId, droppedCommitId];
    const gone = watched.find(
      (commitId) => commitId != null && !stashes.some((stash) => stash.commitId === commitId),
    );
    if (gone != null) {
      setRefocus({ position: Math.max(0, lastStashes.findIndex((stash) => stash.commitId === gone)) });
      if (gone === droppedCommitId) setDroppedCommitId(null);
    }
  }
  if (menu !== null && menuStash === undefined) setMenu(null);
  if (pendingDrop !== null && dropStash === undefined) setPendingDrop(null);
  // The list, or the empty-state text when there is none.
  const bodyRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (refocus === null) return;
    const active = document.activeElement;
    if (active !== null && active !== document.body && active.isConnected) return;
    // The "…" of the stash now in that place, else the last one, else the section's header.
    const buttons = bodyRef.current?.querySelectorAll<HTMLElement>('button[aria-haspopup="menu"]') ?? [];
    const next = buttons.length > 0 ? buttons[Math.min(refocus.position, buttons.length - 1)] : undefined;
    const header = bodyRef.current?.closest("section")?.querySelector<HTMLElement>("button");
    (next ?? header)?.focus({ preventScroll: true });
  }, [refocus]);
  const dropDisabledReason = blockers.drop;
  const dropDisabled = dropDisabledReason !== null;
  const dropTitle = dropDisabledReason ?? undefined;

  // A new menu or Drop dialog ends watching an earlier drop (its stash may never go).
  const openMenu = (next: StashMenu) => {
    setDroppedCommitId(null);
    setMenu(next);
  };

  const menuItems = (stash: StashEntry): ContextMenuItem[] => {
    const actions = stashActions(blockers, stash);
    return [
      menuItem(actions.apply, () => onApplyStash(stash.index)),
      menuItem(actions.pop, () => onPopStash(stash.index)),
      {
        ...menuItem(actions.drop, () => {
          setDroppedCommitId(null);
          setPendingDrop(stash);
        }),
        destructive: true,
      },
    ];
  };

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
                onOpenMenu={(anchor, trigger) => openMenu({ stash, ...anchor, returnFocusTo: trigger })}
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
          // Opened from the row by a key or a right-click, the row's "…" is still its own button.
          isTrigger={(button) => button.getAttribute("data-menu-key") === menuStash.commitId}
          onClose={() => setMenu(null)}
          items={menuItems(menuStash)}
        />
      )}
      {dropStash !== undefined && (
        <ConfirmDialog
          ariaLabel={`Drop stash ${dropStash.message}`}
          message={
            <>
              <p>Drop "{dropStash.message}"? This cannot be undone.</p>
              {/* #521: the repository can become busy while the confirmation is open. */}
              {dropDisabledReason !== null && <p className={formStyles.blocked}>{dropDisabledReason}</p>}
            </>
          }
          confirmLabel="Drop stash"
          confirmDisabled={dropDisabled}
          confirmTitle={dropTitle}
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
