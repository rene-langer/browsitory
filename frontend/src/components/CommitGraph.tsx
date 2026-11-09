import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import type { GraphCommit, HeadInfo, RemoteInfo, StatusEntry } from "../ipc/RepoClient";
import { assignLanes } from "../lib/commitGraphLayout";
import {
  EMPTY_SELECTION,
  extendSelection,
  selectOnly,
  toggleCommit,
  type CommitSelection,
} from "../lib/commitSelection";
import {
  planRebaseSelection,
  presetForAction,
  presetPickAll,
  type PresetAction,
} from "../lib/rebaseSelection";
import { historyEditChecks, type HistoryEditCheck, type HistoryEditRequest } from "../lib/historyEdits";
import { commitActions } from "../lib/actions/commitActions";
import { rowsEqual, type SelectedRow } from "../state/useAppState";
import { CommitLaneGraphic, WorkingTreeLaneGraphic } from "./CommitLaneGraphic";
import { formatShortDate } from "../lib/formatDate";
import { ListRow } from "./primitives/ListRow";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import { isMenuKey, menuAnchor } from "../lib/menuKeys";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import formStyles from "./primitives/FormDialog.module.css";
import { RefBadges } from "./RefBadges";
import { tagMenuItems } from "./tagMenuItems";
import styles from "./CommitGraph.module.css";

const PAGE_SIZE = 10;

export function CommitGraph({
  status,
  commits,
  selectedRow,
  pending,
  pendingReason = null,
  onSelectRow,
  onBranchFromCommit,
  onRebaseFromCommit,
  onRebaseSelection,
  headInfo,
  currentBranch,
  onCherryPick,
  onRevert,
  onCherryPickMany,
  onRevertMany,
  onCheckoutCommit,
  onChooseParent,
  onRequestReset,
  onEditHeadMessage,
  onEditHistory,
  onRewordCommit,
  hasMore = false,
  onLoadMore,
  graphRemoteBranchSelection = null,
  onRequestSearch,
  onCompareCommits,
  onCompareWithWorkingTree,
  onSavePatch,
  remotes = [],
  onPushTag,
  onDeleteTag,
  onCreateTag,
  onDeleteTagFromRemote,
  revealRequest = 0,
}: {
  status: StatusEntry[];
  commits: GraphCommit[];
  selectedRow: SelectedRow;
  // True while repository operations are blocked (`operationBlockers`): disables the menus'
  // mutating entries.
  pending: boolean;
  /** `operationBlockers`' reason, the title of the entries `pending` disables. */
  pendingReason?: string | null;
  onSelectRow: (row: SelectedRow) => void;
  onBranchFromCommit: (commitId: string) => void;
  onRebaseFromCommit: (commitId: string) => void;
  // Called when the user acts on a multi-commit selection (Interactive rebase…, Squash, Fixup,
  // Drop). `onto` is the oldest selected commit's own parent — the base the planner rebases onto;
  // `preset` maps commit ids to the action to pre-mark in the planner (every selected
  // id as "Pick" for a plain Interactive rebase…, so the planner still checks they are on the branch).
  onRebaseSelection?: (onto: string, preset: ReadonlyMap<string, PresetAction>) => void;
  headInfo: HeadInfo;
  // Name of the checked-out branch; `null` on a detached HEAD (reset is then unavailable).
  currentBranch: string | null;
  // A non-merge commit's pick or revert (`mainline` is always `null` here).
  onCherryPick: (commitId: string, mainline: number | null) => void;
  onRevert: (commitId: string, mainline: number | null) => void;
  /** Multi-selection "Cherry-pick N commits" / "Revert N commits"; ids oldest-first. */
  onCherryPickMany?: (commitIdsOldestFirst: string[]) => void;
  onRevertMany?: (commitIdsOldestFirst: string[]) => void;
  onCheckoutCommit: (commitId: string) => void;
  /**
   * A merge commit's Cherry-pick… / Revert…: opens the parent picker. Like the reset dialog it
   * is rendered by `RepoWorkspace`, the one instance the command palette opens too (#514);
   * `returnFocusTo` is this list (#146).
   */
  onChooseParent: (action: "Cherry-pick" | "Revert", commitId: string, returnFocusTo: HTMLElement | null) => void;
  /** "Reset <branch> to here…": opens the reset dialog (see `onChooseParent`). */
  onRequestReset: (commitId: string, returnFocusTo: HTMLElement | null) => void;
  /** HEAD's Reword…: opens the amend box with its message. */
  onEditHeadMessage: () => void;
  /**
   * The one-click edits of a commit of the current branch (#442): Squash with parent, Fixup
   * into parent, Move up/down, Drop, each a one-step rebase from `onto`. Without it, the
   * menu offers only HEAD's Reword….
   */
  onEditHistory?: (commitId: string, onto: string, request: HistoryEditRequest) => void;
  /** Reword… of a commit below HEAD: opens the Reword dialog (a one-step rebase from `onto`). */
  onRewordCommit?: (commitId: string, onto: string) => void;
  // True when the loaded history filled its limit, so older commits probably exist.
  hasMore?: boolean;
  onLoadMore?: () => void;
  // `null`/omitted means "show every remote badge present" — mirrors BranchTree's local
  // `graphBranchSelection ?? branches.map(...)` fallback, but computed here from the commits
  // actually on screen: `null` walks no remote branch, so there is no list to derive it from.
  // Non-null: those remote branches' commits are in the graph too (#385), only their badges show.
  graphRemoteBranchSelection?: string[] | null;
  /** Ctrl/Cmd+F while the list has focus: focus the commit search field. */
  onRequestSearch?: () => void;
  // Read-only compare actions: never disabled by `pending`.
  /** Exactly two commits selected: compare `fromId` (further down the graph) with `toId`. */
  onCompareCommits?: (fromId: string, toId: string) => void;
  /** Single-commit menu: compare that commit with the working tree. */
  onCompareWithWorkingTree?: (commitId: string) => void;
  /** "Save as patch…" for the commit, or the selected commits (#448); ids oldest-first. Read-only. */
  onSavePatch?: (commitIdsOldestFirst: string[]) => void;
  /** Push targets offered by a tag badge's menu. */
  remotes?: RemoteInfo[];
  /**
   * Tag badge menu actions (push, branch, checkout, delete locally or on a remote). Without
   * `onPushTag`, tag badges have no menu and a right-click on them opens the commit menu. Badges
   * stay non-focusable; `TagPanel` is the keyboard path for push and delete, and the commit menu
   * has "Branch from here" and "Checkout (detached)".
   */
  onPushTag?: (remoteName: string, tag: string) => void;
  onDeleteTag?: (tag: string) => Promise<void>;
  /** Opens the create-tag dialog for that commit (the single-commit menu's "Create tag…"). */
  onCreateTag?: (commitId: string) => void;
  /** Opens the "Delete from remote" dialog for that tag (the tag badge menu). */
  onDeleteTagFromRemote?: (tag: string) => void;
  /** Bumped by "Go to" (#441): scrolls the selected row into view even when it was already
   *  selected. */
  revealRequest?: number;
}) {
  const [contextMenu, setContextMenu] = useState<{
    commitId: string;
    x: number;
    y: number;
  } | null>(null);
  const [tagMenu, setTagMenu] = useState<{ tag: string; commitId: string; x: number; y: number } | null>(null);
  const [tagToDelete, setTagToDelete] = useState<string | null>(null);
  const [hoveredSegmentId, setHoveredSegmentId] = useState<number | null>(null);
  // `focusId` is the last focus the graph itself reported through `onSelectRow`. A focus that
  // differs from it came from outside (search, sidebar, refresh) and drops the multi-selection.
  const [{ selection, focusId }, setGraphSelection] = useState<{
    selection: CommitSelection;
    focusId: string | null;
  }>({ selection: EMPTY_SELECTION, focusId: null });

  const rows: SelectedRow[] = [
    "uncommitted",
    ...commits.map((commit) => ({ commitId: commit.id })),
  ];
  const selectedIndex = rows.findIndex((row) => rowsEqual(row, selectedRow));

  // The multi-selection only counts while the focused commit is the one the graph last set and
  // belongs to it, and only for commits still loaded. Anything else (a click outside the graph,
  // search, the Uncommitted row, a refresh) collapses or prunes it without an effect keeping two
  // sources of truth in sync.
  const primaryId = typeof selectedRow === "object" ? selectedRow.commitId : null;
  const effectiveSelection: CommitSelection = (() => {
    if (primaryId === null) return EMPTY_SELECTION;
    if (primaryId !== focusId) return selectOnly(primaryId);
    const loaded = new Set(commits.map((commit) => commit.id));
    const ids = new Set([...selection.ids].filter((id) => loaded.has(id)));
    if (!ids.has(primaryId)) return selectOnly(primaryId);
    if (ids.size === selection.ids.size) return selection;
    const anchorId = selection.anchorId !== null && ids.has(selection.anchorId) ? selection.anchorId : primaryId;
    return { ids, anchorId };
  })();

  const setSelection = (next: CommitSelection, row: SelectedRow) => {
    setGraphSelection({ selection: next, focusId: typeof row === "object" ? row.commitId : null });
    onSelectRow(row);
  };

  const listRef = useRef<HTMLUListElement>(null);
  // The same element as state, for the menus' `returnFocusTo` (a ref can't be read in render):
  // focus goes from a chosen item straight to the list, never back through whatever was focused
  // before the menu opened (#146).
  const [listElement, setListElement] = useState<HTMLUListElement | null>(null);
  const attachList = useCallback((element: HTMLUListElement | null) => {
    listRef.current = element;
    setListElement(element);
  }, []);

  const busyTitle = pending ? (pendingReason ?? undefined) : undefined;

  function singleCommitItems(commitId: string): ContextMenuItem[] {
    const commit = commits.find((candidate) => candidate.id === commitId);
    const isHead = headInfo.headId === commitId;
    const items: ContextMenuItem[] = [{ label: "Branch from here", onSelect: () => onBranchFromCommit(commitId) }];
    if (commit !== undefined) {
      // Availability, reasons and labels shared with the command palette (#495).
      const actions = commitActions(
        { blockers: { blocked: pending, reason: pendingReason }, headId: headInfo.headId, currentBranch },
        commit,
      );
      const menuItem = (action: { label: string; enabled: boolean; reason: string | undefined }, run: () => void) => ({
        label: action.label,
        onSelect: run,
        disabled: !action.enabled,
        title: action.reason,
      });
      if (onCreateTag !== undefined) items.push(menuItem(actions.createTag, () => onCreateTag(commitId)));
      items.push(
        { label: "Rebase onto here", onSelect: () => onRebaseFromCommit(commitId), disabled: pending, title: busyTitle },
        // A merge commit needs a parent to apply against: the ellipsis opens the parent picker.
        menuItem(actions.cherryPick, () =>
          actions.cherryPick.needsParent
            ? onChooseParent("Cherry-pick", commitId, listElement)
            : onCherryPick(commitId, null),
        ),
        menuItem(actions.revert, () =>
          actions.revert.needsParent
            ? onChooseParent("Revert", commitId, listElement)
            : onRevert(commitId, null),
        ),
        menuItem(actions.checkout, () => onCheckoutCommit(commitId)),
        menuItem(actions.reset, () => onRequestReset(commitId, listElement)),
      );
    }
    items.push(...editItems(commitId, isHead));
    if (onCompareWithWorkingTree !== undefined) {
      items.push({ label: "Diff against working tree", onSelect: () => onCompareWithWorkingTree(commitId) });
    }
    if (onSavePatch !== undefined) {
      items.push({ label: "Save as patch…", onSelect: () => onSavePatch([commitId]) });
    }
    return items;
  }

  // One-click history edits (#442). HEAD's Reword… amends, so it needs neither a clean tree nor
  // a parent, as "Edit commit message…" before it.
  function editItems(commitId: string, isHead: boolean): ContextMenuItem[] {
    const headReword: ContextMenuItem = {
      label: "Reword…",
      onSelect: onEditHeadMessage,
      disabled: pending,
      title: busyTitle,
    };
    if (onEditHistory === undefined) return isHead ? [headReword] : [];
    const checks = historyEditChecks({ commits, headInfo, currentBranch, status }, commitId);
    const item = (label: string, check: HistoryEditCheck, run: (onto: string) => void): ContextMenuItem => ({
      label,
      onSelect: () => {
        if (check.ok) run(check.onto);
      },
      disabled: pending || !check.ok,
      title: check.ok ? busyTitle : check.reason,
    });
    const edit = (request: HistoryEditRequest) => (onto: string) => onEditHistory(commitId, onto, request);
    return [
      isHead ? headReword : item("Reword…", checks.Reword, (onto) => onRewordCommit?.(commitId, onto)),
      item("Squash with parent", checks.Squash, edit({ kind: "Squash" })),
      item("Fixup into parent", checks.Fixup, edit({ kind: "Fixup" })),
      item("Move up", checks.MoveUp, edit({ kind: "MoveUp" })),
      item("Move down", checks.MoveDown, edit({ kind: "MoveDown" })),
      { ...item("Drop", checks.Drop, edit({ kind: "Drop" })), destructive: true },
    ];
  }

  function tagItems(tag: string, commitId: string): ContextMenuItem[] {
    return tagMenuItems({
      tag,
      commitId,
      remotes,
      disabled: pending,
      disabledReason: pendingReason,
      onPushTag,
      onBranchFromCommit,
      onCheckoutCommit,
      onDeleteFromRemote: onDeleteTagFromRemote,
      onDelete: setTagToDelete,
    });
  }

  // Keep the keyboard-driven selection visible in a long history, and a "Go to" target too.
  useEffect(() => {
    const selected = listRef.current?.children[selectedIndex] as HTMLElement | undefined;
    if (typeof selected?.scrollIntoView === "function") selected.scrollIntoView({ block: "nearest" });
  }, [selectedIndex, revealRequest]);

  const moveSelection = (nextIndex: number, extendRange: boolean) => {
    // Arrowing/paging past the last loaded row pulls in the next page instead of dead-ending.
    if (nextIndex > rows.length - 1 && hasMore) onLoadMore?.();
    const next = Math.max(0, Math.min(nextIndex, rows.length - 1));
    if (next < 1) {
      setSelection(EMPTY_SELECTION, rows[next]);
    } else if (extendRange) {
      // Row 0 is "Uncommitted changes"; commit rows are offset by one.
      setSelection(extendSelection(effectiveSelection, commits, commits[next - 1].id), rows[next]);
    } else {
      setSelection(selectOnly(commits[next - 1].id), rows[next]);
    }
  };

  const openMenuForSelected = (list: HTMLUListElement) => {
    if (selectedIndex < 1) return;
    const row = list.children[selectedIndex] as HTMLElement | undefined;
    const rect = row?.getBoundingClientRect();
    setContextMenu({
      commitId: commits[selectedIndex - 1].id,
      x: rect?.left ?? 0,
      y: rect?.bottom ?? 0,
    });
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    // Keys from the open context menu (or any nested control) are not list navigation.
    if (event.target !== event.currentTarget) return;
    if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "f" && onRequestSearch) {
      event.preventDefault();
      onRequestSearch();
      return;
    }
    const extend = event.shiftKey;
    if (event.key === "ArrowDown" || event.key === "j") {
      event.preventDefault();
      moveSelection(selectedIndex + 1, extend);
    } else if (event.key === "ArrowUp" || event.key === "k") {
      event.preventDefault();
      moveSelection(selectedIndex - 1, extend);
    } else if (event.key === "Home") {
      event.preventDefault();
      moveSelection(0, false);
    } else if (event.key === "End") {
      event.preventDefault();
      moveSelection(rows.length - 1, false);
    } else if (event.key === "PageDown") {
      event.preventDefault();
      moveSelection(selectedIndex + PAGE_SIZE, false);
    } else if (event.key === "PageUp") {
      event.preventDefault();
      moveSelection(selectedIndex - PAGE_SIZE, false);
    } else if (event.key === "Escape") {
      if (effectiveSelection.ids.size > 1 && primaryId !== null) {
        event.preventDefault();
        setSelection(selectOnly(primaryId), { commitId: primaryId });
      }
    } else if (event.key === "Enter" || isMenuKey(event)) {
      event.preventDefault();
      openMenuForSelected(event.currentTarget);
    }
  };

  const handleContextMenu = (event: MouseEvent, commitId: string) => {
    event.preventDefault();
    setContextMenu({ commitId, ...menuAnchor(event, event.currentTarget as Element) });
  };

  const handleCommitClick = (event: MouseEvent | undefined, commitId: string) => {
    if (event?.ctrlKey || event?.metaKey) {
      const next = toggleCommit(effectiveSelection, commitId);
      if (next.ids.size === 0) return; // the only selected commit can't be toggled off
      // Keep focus where it is unless the focused commit itself was toggled off.
      const focus = next.ids.has(commitId)
        ? commitId
        : primaryId !== null && next.ids.has(primaryId)
          ? primaryId
          : (next.anchorId ?? commitId);
      setSelection(next, { commitId: focus });
      return;
    }
    setSelection(
      event?.shiftKey ? extendSelection(effectiveSelection, commits, commitId) : selectOnly(commitId),
      { commitId },
    );
  };

  const multiSelected = effectiveSelection.ids.size >= 2;
  const rebasePlan = multiSelected
    ? planRebaseSelection(commits, effectiveSelection.ids)
    : null;

  // The same actions back the context menu and the toolbar under the list.
  const rebaseItems: ContextMenuItem[] = (() => {
    if (rebasePlan === null) return [];
    const count = effectiveSelection.ids.size;
    if (!rebasePlan.ok) {
      const blocked = (label: string): ContextMenuItem => ({
        label,
        onSelect: () => {},
        disabled: true,
        title: rebasePlan.reason,
      });
      return [
        blocked("Interactive rebase…"),
        blocked(`Squash ${count} commits`),
        blocked(`Fixup ${count} commits`),
        blocked(`Drop ${count} commits`),
      ];
    }
    const { selection: plan } = rebasePlan;
    const needsRun = "Squash and Fixup need commits that are next to each other.";
    const act = (action: Exclude<PresetAction, "Pick">) => () =>
      onRebaseSelection?.(plan.onto, presetForAction(plan, action));
    return [
      {
        label: "Interactive rebase…",
        onSelect: () => onRebaseSelection?.(plan.onto, presetPickAll(plan)),
        disabled: pending,
      },
      {
        label: `Squash ${count} commits`,
        onSelect: act("Squash"),
        disabled: pending || !plan.contiguous,
        title: plan.contiguous ? undefined : needsRun,
      },
      {
        label: `Fixup ${count} commits`,
        onSelect: act("Fixup"),
        disabled: pending || !plan.contiguous,
        title: plan.contiguous ? undefined : needsRun,
      },
      {
        label: `Drop ${count} commits`,
        onSelect: act("Drop"),
        disabled: pending,
        destructive: true,
      },
    ];
  })();

  // Read-only, so neither `pending` nor a blocked rebase plan disables it.
  const compareItem: ContextMenuItem | null = (() => {
    if (onCompareCommits === undefined || effectiveSelection.ids.size !== 2) return null;
    const [first, second] = [...effectiveSelection.ids];
    // Graph order is newest first, so the higher index is the older commit.
    const firstIndex = commits.findIndex((c) => c.id === first);
    const secondIndex = commits.findIndex((c) => c.id === second);
    const [fromId, toId] = firstIndex > secondIndex ? [first, second] : [second, first];
    return { label: "Compare these commits", onSelect: () => onCompareCommits(fromId, toId) };
  })();
  // Several commits as one cherry-pick/revert sequence. Unlike the rebase items they don't need
  // a rebasable plan (root commits are fine), only no merge commit: one mainline can't sensibly
  // apply to several merges.
  const pickItems: ContextMenuItem[] = (() => {
    if (!multiSelected || (onCherryPickMany === undefined && onRevertMany === undefined)) return [];
    const count = effectiveSelection.ids.size;
    // Graph order is newest first; the backend wants oldest first.
    const selected = commits.filter((commit) => effectiveSelection.ids.has(commit.id)).reverse();
    const idsOldestFirst = selected.map((commit) => commit.id);
    const hasMerge = selected.some((commit) => commit.parentIds.length > 1);
    const items: ContextMenuItem[] = [];
    if (onCherryPickMany !== undefined) {
      items.push({
        label: `Cherry-pick ${count} commits`,
        onSelect: () => onCherryPickMany(idsOldestFirst),
        disabled: pending || hasMerge,
        title: hasMerge ? "Cherry-pick merge commits one at a time." : undefined,
      });
    }
    if (onRevertMany !== undefined) {
      items.push({
        label: `Revert ${count} commits`,
        onSelect: () => onRevertMany(idsOldestFirst),
        disabled: pending || hasMerge,
        title: hasMerge ? "Revert merge commits one at a time." : undefined,
      });
    }
    return items;
  })();
  // Read-only too: a mailbox of the selected commits, oldest first.
  const savePatchItem: ContextMenuItem | null =
    onSavePatch === undefined || !multiSelected
      ? null
      : {
          label: `Save ${effectiveSelection.ids.size} commits as patch…`,
          onSelect: () =>
            onSavePatch(
              commits
                .filter((commit) => effectiveSelection.ids.has(commit.id))
                .reverse()
                .map((commit) => commit.id),
            ),
        };
  const multiSelectItems: ContextMenuItem[] = [
    ...rebaseItems,
    ...(compareItem === null ? [] : [compareItem]),
    ...pickItems,
    ...(savePatchItem === null ? [] : [savePatchItem]),
  ];

  const commitLayouts = useMemo(() => assignLanes(commits), [commits]);
  // `null` means "show every remote badge" — skip building the membership set entirely in that
  // case rather than deriving one from `commits` just to have every `.includes()` check pass.
  const shownRemoteBranches = graphRemoteBranchSelection === null ? null : new Set(graphRemoteBranchSelection);
  const laneCount =
    Math.max(
      0,
      ...commitLayouts.map((l) => l.lane),
      ...commitLayouts.flatMap((l) => l.passThroughLanes.map((entry) => entry.lane)),
      ...commitLayouts.flatMap((l) => l.parentConnections.map((c) => c.lane)),
    ) + 1;

  return (
    <>
    <ul
      ref={attachList}
      className={styles.list}
      onKeyDown={handleKeyDown}
      tabIndex={0}
      role="listbox"
      aria-multiselectable="true"
      aria-label="Commit history"
      aria-keyshortcuts="ArrowUp ArrowDown Home End PageUp PageDown Shift+ArrowUp Shift+ArrowDown Enter ContextMenu Shift+F10 Escape Control+F Meta+F"
    >
      <ListRow
        className={styles.uncommittedRow}
        selected={selectedRow === "uncommitted"}
        onClick={() => {
          setSelection(EMPTY_SELECTION, "uncommitted");
        }}
      >
        <div className={styles.graphCell}>
          <WorkingTreeLaneGraphic lane={commitLayouts[0]?.lane ?? 0} totalLanes={laneCount} />
        </div>
        <span className={styles.commitSummary}>
          Uncommitted changes{status.length > 0 && ` (${status.length})`}
        </span>
      </ListRow>
      {commits.map((commit, index) => (
        <ListRow
          key={commit.id}
          className="commit-row"
          data-commit-id={commit.id}
          selected={effectiveSelection.ids.has(commit.id)}
          onClick={(event) => handleCommitClick(event, commit.id)}
          onContextMenu={(event) => handleContextMenu(event, commit.id)}
          onMouseEnter={() => setHoveredSegmentId(commitLayouts[index].laneSegmentId)}
          onMouseLeave={() => setHoveredSegmentId(null)}
        >
          <div className={styles.graphCell}>
            <CommitLaneGraphic
              layout={commitLayouts[index]}
              totalLanes={laneCount}
              hoveredSegmentId={hoveredSegmentId}
            />
          </div>
          <RefBadges
            commit={commit}
            detachedHead={headInfo.detachedAt !== null && headInfo.headId === commit.id}
            shownRemoteBranches={shownRemoteBranches}
            onTagContextMenu={
              onPushTag === undefined
                ? undefined
                : (tag, event) => setTagMenu({ tag, commitId: commit.id, ...menuAnchor(event, event.currentTarget as Element) })
            }
          />
          <span className={styles.commitSummary}>
            {commit.shortId} {commit.summary}
          </span>
          <span className={styles.author} title={commit.authorEmail}>
            {commit.authorName}
          </span>
          <span
            className={styles.date}
            data-testid="commit-date"
            title={new Date(commit.timestamp * 1000).toLocaleString()}
          >
            {formatShortDate(commit.timestamp)}
          </span>
        </ListRow>
      ))}
      {contextMenu !== null && (
        <ContextMenu
          key={contextMenu.commitId}
          returnFocusTo={listElement}
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => {
            setContextMenu(null);
            listRef.current?.focus();
          }}
          items={
            multiSelected && effectiveSelection.ids.has(contextMenu.commitId)
              ? multiSelectItems
              : singleCommitItems(contextMenu.commitId)
          }
        />
      )}
      {tagMenu !== null && (
        <ContextMenu
          key={`${tagMenu.commitId}:${tagMenu.tag}`}
          returnFocusTo={listElement}
          x={tagMenu.x}
          y={tagMenu.y}
          onClose={() => {
            setTagMenu(null);
            listRef.current?.focus();
          }}
          items={tagItems(tagMenu.tag, tagMenu.commitId)}
        />
      )}
    </ul>
    {tagToDelete !== null && (
      <ConfirmDialog
        ariaLabel={`Delete local tag ${tagToDelete}`}
        message={
          <>
            <p>Delete local tag {tagToDelete}?</p>
            {/* #521: the repository can become busy while the confirmation is open. */}
            {pending && pendingReason !== null && <p className={formStyles.blocked}>{pendingReason}</p>}
          </>
        }
        confirmLabel="Delete tag"
        confirmDisabled={pending}
        confirmTitle={busyTitle}
        onConfirm={() => {
          if (pending) return;
          void onDeleteTag?.(tagToDelete).then(() => setTagToDelete(null));
        }}
        onCancel={() => setTagToDelete(null)}
      />
    )}
    {multiSelected && (
      <div className={styles.selectionBar} role="toolbar" aria-label="Selected commits">
        <span>{effectiveSelection.ids.size} commits selected</span>
        {multiSelectItems.map((item) => (
          <button
            key={item.label}
            type="button"
            onClick={item.onSelect}
            disabled={item.disabled}
            title={item.title}
          >
            {item.label}
          </button>
        ))}
      </div>
    )}
    {hasMore && (
      <div className={styles.loadMore}>
        <span>Showing latest {commits.length} commits</span>
        <button type="button" onClick={() => onLoadMore?.()}>
          Load more
        </button>
      </div>
    )}
    </>
  );
}
