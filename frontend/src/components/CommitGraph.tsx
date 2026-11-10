import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import type { CommitStats, GraphCommit, HeadInfo, RemoteInfo, StatusEntry } from "../ipc/RepoClient";
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
import type { VisitKind } from "../lib/navigationHistory";
import { rowsEqual, type SelectedRow } from "../state/useAppState";
import { useCopyToClipboard } from "../state/useCopyToClipboard";
import { CommitLaneGraphic, WorkingTreeLaneGraphic } from "./CommitLaneGraphic";
import { formatShortDate } from "../lib/formatDate";
import { ListRow } from "./primitives/ListRow";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import { isMenuKey, menuAnchor } from "../lib/menuKeys";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import formStyles from "./primitives/FormDialog.module.css";
import { LineStatsBadge } from "./LineStatsBadge";
import { expandedFrom, expansionHasMore, MERGE_PAGE_SIZE, type MergeExpansion } from "../lib/mergeExpansion";
import { RefBadges } from "./RefBadges";
import { tagMenuItems } from "./tagMenuItems";
import styles from "./CommitGraph.module.css";

const PAGE_SIZE = 10;
const NO_EXPANSIONS: Readonly<Record<string, MergeExpansion>> = {};

export function CommitGraph({
  status,
  commits,
  selectedRow,
  operationsBlocked,
  operationsBlockedReason = null,
  onSelectRow,
  onGoToCommit,
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
  lineStats,
  onCommitsVisible,
  expandedMerges = NO_EXPANSIONS,
  onExpandMerge,
  onCollapseMerge,
}: {
  status: StatusEntry[];
  commits: GraphCommit[];
  selectedRow: SelectedRow;
  // True while repository operations are blocked (`operationBlockers`): disables the menus'
  // mutating entries.
  operationsBlocked: boolean;
  /** `operationBlockers`' reason, the title of the entries `operationsBlocked` disables. */
  operationsBlockedReason?: string | null;
  /** `kind` `"step"` for a keyboard move by row (Back/Forward merge those, #618). */
  onSelectRow: (row: SelectedRow, kind?: VisitKind) => void;
  /** Selects a parent outside the loaded history, as Go to does (#618). */
  onGoToCommit?: (commitId: string) => void;
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
  // Read-only compare actions: never disabled by `operationsBlocked`.
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
  /**
   * Lines-changed badges (#620): the counted commits (`useCommitLineStats`); a row shows its
   * badge once its numbers are in, and none for a commit over the cap (`null`). Omitted: no
   * badges.
   */
  lineStats?: ReadonlyMap<string, CommitStats | null>;
  /** The commits whose rows are on screen, in graph order, whenever that changes. */
  onCommitsVisible?: (commitIds: string[]) => void;
  /**
   * Collapsed merges (#617): the expanded ones, whose commits `commits` already lists below them.
   * A row with `mergedCount` gets a "+N" control (and ArrowRight/ArrowLeft on the selected row)
   * when `onExpandMerge` is given; `atLeast` is how many of its commits to load.
   */
  expandedMerges?: Readonly<Record<string, MergeExpansion>>;
  onExpandMerge?: (mergeId: string, atLeast?: number) => void;
  onCollapseMerge?: (mergeId: string) => void;
}) {
  const [contextMenu, setContextMenu] = useState<{
    commitId: string;
    x: number;
    y: number;
  } | null>(null);
  const [tagMenu, setTagMenu] = useState<{ tag: string; commitId: string; x: number; y: number } | null>(null);
  const [tagToDelete, setTagToDelete] = useState<string | null>(null);
  const copyToClipboard = useCopyToClipboard();
  // The parent or child picker of the parent/child keys (#618).
  const [navigationPicker, setNavigationPicker] = useState<{ items: ContextMenuItem[]; x: number; y: number } | null>(
    null,
  );
  // The commit of an expanded merge each listed commit belongs to (#617).
  const mergeOwners = useMemo(() => expandedFrom(commits, expandedMerges), [commits, expandedMerges]);
  const commitLayouts = useMemo(() => assignLanes(commits, new Set(mergeOwners.keys())), [commits, mergeOwners]);
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

  const setSelection = (next: CommitSelection, row: SelectedRow, kind?: VisitKind) => {
    setGraphSelection({ selection: next, focusId: typeof row === "object" ? row.commitId : null });
    if (kind === undefined) onSelectRow(row);
    else onSelectRow(row, kind);
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

  const busyTitle = operationsBlocked ? (operationsBlockedReason ?? undefined) : undefined;

  function singleCommitItems(commitId: string): ContextMenuItem[] {
    const commit = commits.find((candidate) => candidate.id === commitId);
    const isHead = headInfo.headId === commitId;
    const items: ContextMenuItem[] = [{ label: "Branch from here", onSelect: () => onBranchFromCommit(commitId) }];
    if (commit !== undefined) {
      // Availability, reasons and labels shared with the command palette (#495).
      const actions = commitActions(
        { blockers: { blocked: operationsBlocked, reason: operationsBlockedReason }, headId: headInfo.headId, currentBranch },
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
        { label: "Rebase onto here", onSelect: () => onRebaseFromCommit(commitId), disabled: operationsBlocked, title: busyTitle },
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
    items.push(...copyItems(commitId, commit));
    return items;
  }

  // "Copy …" (#614): last in the menu and never disabled (copying is not a Git operation). The
  // date is the instant the commit details show, in ISO 8601.
  function copyItems(commitId: string, commit: GraphCommit | undefined): ContextMenuItem[] {
    const items: ContextMenuItem[] = [{ label: "Copy SHA", onSelect: () => copyToClipboard(commitId, "SHA") }];
    if (commit === undefined) return items;
    const author = `${commit.authorName} <${commit.authorEmail}>`;
    const date = new Date(commit.timestamp * 1000).toISOString();
    items.push(
      { label: "Copy short SHA", onSelect: () => copyToClipboard(commit.shortId, "short SHA") },
      { label: "Copy subject", onSelect: () => copyToClipboard(commit.summary, "subject") },
      { label: "Copy author", onSelect: () => copyToClipboard(author, "author") },
      { label: "Copy date", onSelect: () => copyToClipboard(date, "date") },
    );
    return items;
  }

  // One-click history edits (#442). HEAD's Reword… amends, so it needs neither a clean tree nor
  // a parent, as "Edit commit message…" before it.
  function editItems(commitId: string, isHead: boolean): ContextMenuItem[] {
    const headReword: ContextMenuItem = {
      label: "Reword…",
      onSelect: onEditHeadMessage,
      disabled: operationsBlocked,
      title: busyTitle,
    };
    if (onEditHistory === undefined) return isHead ? [headReword] : [];
    const checks = historyEditChecks({ commits, headInfo, currentBranch, status }, commitId);
    const item = (label: string, check: HistoryEditCheck, run: (onto: string) => void): ContextMenuItem => ({
      label,
      onSelect: () => {
        if (check.ok) run(check.onto);
      },
      disabled: operationsBlocked || !check.ok,
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
      disabled: operationsBlocked,
      disabledReason: operationsBlockedReason,
      onPushTag,
      onBranchFromCommit,
      onCheckoutCommit,
      onDeleteFromRemote: onDeleteTagFromRemote,
      onCopy: copyToClipboard,
      onDelete: setTagToDelete,
    });
  }

  // The rows on screen, for the lazily counted badges (#620). Without IntersectionObserver every
  // loaded row counts as visible.
  useEffect(() => {
    const list = listRef.current;
    if (onCommitsVisible === undefined || list === null) return;
    if (typeof IntersectionObserver === "undefined") {
      onCommitsVisible(commits.map((commit) => commit.id));
      return;
    }
    const visible = new Set<string>();
    const observer = new IntersectionObserver((records) => {
      for (const record of records) {
        const id = (record.target as HTMLElement).dataset.commitId;
        if (id === undefined) continue;
        if (record.isIntersecting) visible.add(id);
        else visible.delete(id);
      }
      onCommitsVisible(commits.filter((commit) => visible.has(commit.id)).map((commit) => commit.id));
    });
    for (const row of list.querySelectorAll<HTMLElement>("[data-commit-id]")) observer.observe(row);
    return () => observer.disconnect();
  }, [commits, onCommitsVisible]);

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
      setSelection(EMPTY_SELECTION, rows[next], "step");
    } else if (extendRange) {
      // Row 0 is "Uncommitted changes"; commit rows are offset by one.
      setSelection(extendSelection(effectiveSelection, commits, commits[next - 1].id), rows[next], "step");
    } else {
      setSelection(selectOnly(commits[next - 1].id), rows[next], "step");
    }
  };

  // Parent and child keys (#618). A loaded commit is selected here; a parent outside the loaded
  // history is found as Go to does.
  const goToCommit = (commitId: string) => {
    if (commits.some((commit) => commit.id === commitId)) setSelection(selectOnly(commitId), { commitId });
    else onGoToCommit?.(commitId);
  };
  const describeCommit = (commitId: string) => {
    const commit = commits.find((candidate) => candidate.id === commitId);
    return commit === undefined ? commitId.slice(0, 7) : `${commit.shortId} ${commit.summary}`;
  };
  const openPicker = (list: HTMLUListElement, items: ContextMenuItem[]) => {
    const rect = (list.children[selectedIndex] as HTMLElement | undefined)?.getBoundingClientRect();
    setNavigationPicker({ items, x: rect?.left ?? 0, y: rect?.bottom ?? 0 });
  };
  // `[`: the first parent; Shift+`[` on a merge commit: a picker of its parents.
  const goToParent = (list: HTMLUListElement, choose: boolean) => {
    if (selectedIndex < 1) return;
    const { parentIds } = commits[selectedIndex - 1];
    if (parentIds.length === 0) return;
    if (!choose || parentIds.length === 1) {
      goToCommit(parentIds[0]);
      return;
    }
    openPicker(
      list,
      parentIds.map((parentId, index) => ({
        label: `Parent ${index + 1}: ${describeCommit(parentId)}`,
        onSelect: () => goToCommit(parentId),
      })),
    );
  };
  // `]`: the loaded child; of several, the one on the same lane, else a picker.
  const goToChild = (list: HTMLUListElement) => {
    if (selectedIndex < 1) return;
    const commitId = commits[selectedIndex - 1].id;
    const lane = commitLayouts[selectedIndex - 1].lane;
    const children = commits.flatMap((commit, index) => (commit.parentIds.includes(commitId) ? [index] : []));
    if (children.length === 0) return;
    const sameLane = children.filter((index) => commitLayouts[index].lane === lane);
    const only = children.length === 1 ? children[0] : sameLane.length === 1 ? sameLane[0] : null;
    if (only !== null) {
      goToCommit(commits[only].id);
      return;
    }
    openPicker(
      list,
      children.map((index) => ({
        label: describeCommit(commits[index].id),
        onSelect: () => goToCommit(commits[index].id),
      })),
    );
  };

  // Collapsed merges (#617). Collapsing a merge whose commits hold the selection selects the merge,
  // so the selection never sits on a row that is gone.
  const canExpand = (commit: GraphCommit) =>
    onExpandMerge !== undefined && commit.mergedCount !== undefined && commit.mergedCount > 0;
  const expandMerge = (mergeId: string) => onExpandMerge?.(mergeId);
  const collapseMerge = (mergeId: string) => {
    const inside = primaryId !== null && primaryId !== mergeId && mergeOwners.get(primaryId) === mergeId;
    if (inside) setSelection(selectOnly(mergeId), { commitId: mergeId });
    onCollapseMerge?.(mergeId);
  };
  // ArrowRight expands the selected merge; ArrowLeft collapses it, or from one of its commits
  // goes to the merge. `true` when the key did something.
  const expandOrCollapseSelected = (expand: boolean): boolean => {
    if (selectedIndex < 1) return false;
    const commit = commits[selectedIndex - 1];
    const expanded = commit.id in expandedMerges;
    if (expand) {
      if (!canExpand(commit) || expanded) return false;
      expandMerge(commit.id);
      return true;
    }
    if (expanded) {
      collapseMerge(commit.id);
      return true;
    }
    const owner = mergeOwners.get(commit.id);
    if (owner === undefined) return false;
    setSelection(selectOnly(owner), { commitId: owner }, "step");
    return true;
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
    // Cmd+[ / Cmd+] are Back/Forward on macOS (`useWorkspaceShortcuts`); AltGr (Ctrl+Alt) types
    // the brackets on some layouts.
    const bracketKey = !event.metaKey && !(event.ctrlKey && !event.altKey);
    if (bracketKey && (event.key === "[" || event.key === "{")) {
      event.preventDefault();
      goToParent(event.currentTarget, event.key === "{" || event.shiftKey);
      return;
    }
    if (bracketKey && event.key === "]") {
      event.preventDefault();
      goToChild(event.currentTarget);
      return;
    }
    if (
      (event.key === "ArrowRight" || event.key === "ArrowLeft") &&
      !event.shiftKey &&
      !event.altKey &&
      !event.ctrlKey &&
      !event.metaKey
    ) {
      if (expandOrCollapseSelected(event.key === "ArrowRight")) event.preventDefault();
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
        disabled: operationsBlocked,
      },
      {
        label: `Squash ${count} commits`,
        onSelect: act("Squash"),
        disabled: operationsBlocked || !plan.contiguous,
        title: plan.contiguous ? undefined : needsRun,
      },
      {
        label: `Fixup ${count} commits`,
        onSelect: act("Fixup"),
        disabled: operationsBlocked || !plan.contiguous,
        title: plan.contiguous ? undefined : needsRun,
      },
      {
        label: `Drop ${count} commits`,
        onSelect: act("Drop"),
        disabled: operationsBlocked,
        destructive: true,
      },
    ];
  })();

  // Read-only, so neither `operationsBlocked` nor a blocked rebase plan disables it.
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
        disabled: operationsBlocked || hasMerge,
        title: hasMerge ? "Cherry-pick merge commits one at a time." : undefined,
      });
    }
    if (onRevertMany !== undefined) {
      items.push({
        label: `Revert ${count} commits`,
        onSelect: () => onRevertMany(idsOldestFirst),
        disabled: operationsBlocked || hasMerge,
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
      aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight Home End PageUp PageDown Shift+ArrowUp Shift+ArrowDown Enter ContextMenu Shift+F10 Escape Control+F Meta+F [ ] Shift+["
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
          className={mergeOwners.has(commit.id) ? `commit-row ${styles.mergedRow}` : "commit-row"}
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
          {canExpand(commit) && (
            <MergeToggle
              mergedCount={commit.mergedCount ?? 0}
              expansion={expandedMerges[commit.id]}
              onExpand={() => expandMerge(commit.id)}
              onCollapse={() => collapseMerge(commit.id)}
              onShowMore={(atLeast) => onExpandMerge?.(commit.id, atLeast)}
            />
          )}
          <CommitRowLineStats stats={lineStats?.get(commit.id)} />
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
      {navigationPicker !== null && (
        <ContextMenu
          returnFocusTo={listElement}
          x={navigationPicker.x}
          y={navigationPicker.y}
          onClose={() => {
            setNavigationPicker(null);
            listRef.current?.focus();
          }}
          items={navigationPicker.items}
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
            {operationsBlocked && operationsBlockedReason !== null && <p className={formStyles.blocked}>{operationsBlockedReason}</p>}
          </>
        }
        confirmLabel="Delete tag"
        confirmDisabled={operationsBlocked}
        confirmTitle={busyTitle}
        onConfirm={() => {
          if (operationsBlocked) return;
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

/**
 * A collapsed merge's "+N" (#617): expands the N commits it brought in below its row, "−N" hides
 * them again; "More" loads the next page of a long side branch. A mouse affordance: the list's
 * ArrowRight/ArrowLeft do the same from the keyboard, so the buttons stay out of the tab order.
 */
function MergeToggle({
  mergedCount,
  expansion,
  onExpand,
  onCollapse,
  onShowMore,
}: {
  mergedCount: number;
  expansion: MergeExpansion | undefined;
  onExpand: () => void;
  onCollapse: () => void;
  onShowMore: (atLeast: number) => void;
}) {
  const commitsWord = mergedCount === 1 ? "commit" : "commits";
  const expanded = expansion !== undefined;
  return (
    <span className={styles.mergeToggle}>
      <button
        type="button"
        tabIndex={-1}
        aria-expanded={expanded}
        aria-label={`${expanded ? "Hide" : "Show"} the ${mergedCount} ${commitsWord} this merge brought in`}
        title={`${expanded ? "Hide" : "Show"} the ${mergedCount} merged ${commitsWord}`}
        onClick={(event) => {
          event.stopPropagation();
          if (expanded) onCollapse();
          else onExpand();
        }}
      >
        {expanded ? "−" : "+"}
        {mergedCount}
      </button>
      {expansion !== undefined && expansionHasMore(expansion) && (
        <button
          type="button"
          tabIndex={-1}
          aria-label={`Show more merged commits (${expansion.commits.length} of ${mergedCount} shown)`}
          onClick={(event) => {
            event.stopPropagation();
            onShowMore(expansion.limit + MERGE_PAGE_SIZE);
          }}
        >
          More
        </button>
      )}
    </span>
  );
}

/** A row's `+12 −3` (#620); nothing until the numbers are in, or for a commit over the cap. */
function CommitRowLineStats({ stats }: { stats: CommitStats | null | undefined }) {
  if (stats == null) return null;
  return <LineStatsBadge insertions={stats.insertions} deletions={stats.deletions} className={styles.lineStats} />;
}
