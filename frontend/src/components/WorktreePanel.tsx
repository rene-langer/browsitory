import { stableMemo } from "../lib/stableMemo";
import { useState } from "react";
import { FolderGit2, GitFork, MoreHorizontal, Plus } from "lucide-react";
import type { BranchInfo, WorktreeInfo } from "../ipc/RepoClient";
import { rectAnchor } from "../lib/menuKeys";
import { menuItem } from "../lib/actions/action";
import { AccordionSection } from "./primitives/AccordionSection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import { ListRow } from "./primitives/ListRow";
import { Toolbar } from "./primitives/Toolbar";
import { ReasonButton } from "./primitives/ReasonButton";
import { disabledReason } from "../lib/actions/action";
import { NewWorktreeDialog } from "./NewWorktreeDialog";
import styles from "./WorktreePanel.module.css";
import { useRefocusRemovedRow } from "./useRefocusRemovedRow";
import { pruneWorktreesAction, worktreeActions } from "../lib/actions/worktreeActions";
import type { OperationBlockers } from "../lib/operationStatus";

// A row's key (#413): two entries can name one folder, while a linked worktree's name is its own.
const rowKey = (worktree: WorktreeInfo) => (worktree.isMain ? "main" : `linked/${worktree.name}`);

// `returnFocusTo`: the row's "…" for a pointer (WebKit focuses neither the row nor the button);
// `undefined` for a key, so `ContextMenu` returns focus to what had it.
type WorktreeMenu = { key: string; x: number; y: number; returnFocusTo?: HTMLElement };

/**
 * The Worktrees section (#1011): the worktrees first, each with a row menu (the "…" button,
 * right-click, the Menu key or Shift+F10) to open it in a tab of its own or, for a linked one,
 * remove it; "+" in the header opens the New worktree dialog.
 */
function WorktreePanelContent({
  worktrees,
  branches,
  onOpenWorktree,
  onCreateWorktree,
  onRemoveWorktree,
  onPruneWorktrees,
  operationDisabled,
  operationDisabledReason,
  revealRequest,
}: {
  worktrees: WorktreeInfo[];
  branches: BranchInfo[];
  onOpenWorktree: (path: string) => Promise<void>;
  // `null` = created; a string = the failure message to show in the dialog. See
  // `useAppState.ts`'s `createWorktree` (issue #30/UX-002).
  onCreateWorktree: (
    name: string,
    path: string,
    branch: string,
    startPoint: string | null,
  ) => Promise<string | null>;
  onRemoveWorktree: (name: string) => Promise<void>;
  onPruneWorktrees: () => Promise<void>;
  operationDisabled: boolean;
  // Human-readable reason `operationDisabled` is true, shown as a `title` on the controls it
  // disables (issue #31/UX-003). `null` when nothing is blocking.
  operationDisabledReason: string | null;
  /** Expands and scrolls to the section (`useSidebarReveal`, #494). */
  revealRequest?: number;
}) {
  const [creating, setCreating] = useState(false);
  const [menu, setMenu] = useState<WorktreeMenu | null>(null);
  const [removeConfirmation, setRemoveConfirmation] = useState<WorktreeInfo | null>(null);
  const [listElement, setListElement] = useState<HTMLUListElement | null>(null);
  // A removed worktree (#1141), or one a refresh removed while its menu was open.
  useRefocusRemovedRow({
    list: listElement,
    watching: [removeConfirmation === null ? null : rowKey(removeConfirmation), menu?.key],
  });
  // The rows' and Prune's availability and labels, shared with the palette (#509).
  const blockers: OperationBlockers = { blocked: operationDisabled, reason: operationDisabledReason };
  const prune = pruneWorktreesAction(blockers);
  const busyReason = operationDisabled ? operationDisabledReason : null;

  // The menu acts on the worktree it was opened for; a refresh that removed it closes the menu.
  const menuWorktree = menu === null ? undefined : worktrees.find((worktree) => rowKey(worktree) === menu.key);
  if (menu !== null && menuWorktree === undefined) setMenu(null);

  const menuItems = (worktree: WorktreeInfo): ContextMenuItem[] => {
    const actions = worktreeActions(blockers, worktree);
    const items: ContextMenuItem[] = [{ ...menuItem(actions.open, () => void onOpenWorktree(worktree.path)), group: "open" }];
    // The main worktree can never be removed, so it isn't offered (#1011).
    if (!worktree.isMain) {
      items.push({ ...menuItem(actions.remove, () => setRemoveConfirmation(worktree)), destructive: true, group: "remove" });
    }
    return items;
  };

  return (
    <AccordionSection
      title="Worktrees"
      storageKey="sidebar-worktrees"
      icon={GitFork}
      count={worktrees.length}
      revealRequest={revealRequest}
      actions={
        <button
          type="button"
          className={styles.addButton}
          aria-label="New worktree…"
          title="New worktree…"
          onClick={() => setCreating(true)}
        >
          <Plus size={14} aria-hidden="true" />
        </button>
      }
    >
      {worktrees.length === 0 ? (
        <p className={styles.empty}>No worktrees. Use + to check out another branch in a folder of its own.</p>
      ) : (
        <ul ref={setListElement} className={styles.list}>
          {worktrees.map((worktree) => {
            const key = rowKey(worktree);
            const menuOpen = menu?.key === key;
            return (
              <ListRow
                key={key}
                onOpenMenu={(anchor, trigger) => setMenu({ key, ...anchor, returnFocusTo: trigger })}
              >
                <FolderGit2 size={14} aria-hidden="true" className={styles.rowIcon} />
                <span className={styles.text}>
                  <span className={styles.name}>
                    {worktree.isMain ? "Main worktree" : worktree.name}
                    <span className={styles.head}>{worktree.head || "Detached HEAD"}</span>
                    {worktree.isLocked && <span className={styles.badge}>Locked</span>}
                    {worktree.isPrunable && <span className={styles.badge}>Prunable</span>}
                  </span>
                  <span className={styles.path} title={worktree.path}>
                    {worktree.path}
                  </span>
                </span>
                <button
                  type="button"
                  className={styles.iconButton}
                  aria-label={`Actions for ${worktree.path}`}
                  aria-haspopup="menu"
                  aria-expanded={menuOpen}
                  data-menu-key={key}
                  onClick={(event) => {
                    const trigger = event.currentTarget;
                    if (menuOpen) {
                      // A second click closes it; focus stays on the button.
                      trigger.focus({ preventScroll: true });
                      setMenu(null);
                    } else {
                      setMenu({ key, ...rectAnchor(trigger), returnFocusTo: trigger });
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
      <Toolbar>
        <ReasonButton reason={disabledReason(prune)} onClick={() => void onPruneWorktrees()}>
          {prune.label}
        </ReasonButton>
      </Toolbar>
      {menu !== null && menuWorktree !== undefined && (
        <ContextMenu
          key={menu.key}
          x={menu.x}
          y={menu.y}
          returnFocusTo={menu.returnFocusTo}
          isTrigger={(button) => button.getAttribute("data-menu-key") === menu.key}
          onClose={() => setMenu(null)}
          items={menuItems(menuWorktree)}
        />
      )}
      {creating && (
        <NewWorktreeDialog
          branches={branches}
          disabled={operationDisabled}
          disabledReason={operationDisabledReason}
          onCreate={async (name, path, branch, startPoint) => {
            const failure = await onCreateWorktree(name, path, branch, startPoint);
            if (failure === null) setCreating(false);
            return failure;
          }}
          onCancel={() => setCreating(false)}
        />
      )}
      {removeConfirmation !== null && (
        <ConfirmDialog
          title={`Remove worktree ${removeConfirmation.path}`}
          message={
            <>
              <p>Remove worktree at {removeConfirmation.path}?</p>
            </>
          }
          confirmLabel="Remove worktree"
          confirmDisabled={operationDisabled}
          // #561: the repository can become busy while the confirmation is open.
          confirmReason={busyReason}
          onConfirm={() => {
            if (operationDisabled) return;
            void onRemoveWorktree(removeConfirmation.name).then(() => setRemoveConfirmation(null));
          }}
          onCancel={() => setRemoveConfirmation(null)}
        />
      )}
    </AccordionSection>
  );
}

/** Renders again only when its data changed (#844): its branch suggestions list every branch. */
export const WorktreePanel = stableMemo(WorktreePanelContent, "WorktreePanel");
