import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import type { GraphCommit, StatusEntry } from "../ipc/RepoClient";
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
import type { SelectedRow } from "../state/useAppState";
import { CommitLaneGraphic, WorkingTreeLaneGraphic } from "./CommitLaneGraphic";
import { formatShortDate } from "../lib/formatDate";
import { ListRow } from "./primitives/ListRow";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import styles from "./CommitGraph.module.css";

const PAGE_SIZE = 10;

function rowsEqual(a: SelectedRow, b: SelectedRow): boolean {
  if (a === "uncommitted" || b === "uncommitted") {
    return a === b;
  }
  return a.commitId === b.commitId;
}

export function CommitGraph({
  status,
  commits,
  selectedRow,
  pending,
  onSelectRow,
  onBranchFromCommit,
  onRebaseFromCommit,
  onRebaseSelection,
  hasMore = false,
  onLoadMore,
  graphRemoteBranchSelection = null,
}: {
  status: StatusEntry[];
  commits: GraphCommit[];
  selectedRow: SelectedRow;
  // True while a repository operation is in flight. Disables the "Rebase onto here" context
  // menu entry below.
  pending: boolean;
  onSelectRow: (row: SelectedRow) => void;
  onBranchFromCommit: (commitId: string) => void;
  onRebaseFromCommit: (commitId: string) => void;
  // Called when the user acts on a multi-commit selection (Interactive rebase…, Squash, Fixup,
  // Drop). `onto` is the oldest selected commit's own parent — the base the planner rebases onto;
  // `preset` maps commit ids to the action to pre-mark in the planner (every selected
  // id as "Pick" for a plain Interactive rebase…, so the planner still checks they are on the branch).
  onRebaseSelection?: (onto: string, preset: ReadonlyMap<string, PresetAction>) => void;
  // True when the loaded history filled its limit, so older commits probably exist.
  hasMore?: boolean;
  onLoadMore?: () => void;
  // `null`/omitted means "show every remote badge present" — mirrors BranchTree's local
  // `graphBranchSelection ?? branches.map(...)` fallback, but computed here from the commits
  // actually on screen since remote selection never narrows which commits are fetched.
  graphRemoteBranchSelection?: string[] | null;
}) {
  const [contextMenu, setContextMenu] = useState<{
    commitId: string;
    x: number;
    y: number;
  } | null>(null);
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

  // Keep the keyboard-driven selection visible in a long history.
  useEffect(() => {
    const selected = listRef.current?.children[selectedIndex] as HTMLElement | undefined;
    if (typeof selected?.scrollIntoView === "function") selected.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const moveSelection = (nextIndex: number, extendRange: boolean) => {
    // Arrowing/paging past the last loaded row pulls in the next page instead of dead-ending.
    if (nextIndex > rows.length - 1 && hasMore) onLoadMore?.();
    const next = Math.max(0, Math.min(nextIndex, rows.length - 1));
    if (next < 1) {
      setSelection(EMPTY_SELECTION, rows[next]);
    } else if (extendRange) {
      // Row 0 is "Uncommitted Changes"; commit rows are offset by one.
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
    } else if (event.key === "Enter" || event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) {
      event.preventDefault();
      openMenuForSelected(event.currentTarget);
    }
  };

  const handleContextMenu = (event: MouseEvent, commitId: string) => {
    event.preventDefault();
    setContextMenu({ commitId, x: event.clientX, y: event.clientY });
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

  // The same four actions back the context menu and the toolbar under the list.
  const multiSelectItems: ContextMenuItem[] = (() => {
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
      ref={listRef}
      className={styles.list}
      onKeyDown={handleKeyDown}
      tabIndex={0}
      role="listbox"
      aria-multiselectable="true"
      aria-label="Commit history"
      aria-keyshortcuts="ArrowUp ArrowDown Home End PageUp PageDown Shift+ArrowUp Shift+ArrowDown Enter ContextMenu Shift+F10 Escape"
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
          Uncommitted Changes{status.length > 0 && ` (${status.length})`}
        </span>
      </ListRow>
      {commits.map((commit, index) => (
        <ListRow
          key={commit.id}
          className="commit-row"
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
          {commit.branchRefs.map((ref) => (
            <span key={ref} className={styles.branchBadge}>
              {ref}
            </span>
          ))}
          {(shownRemoteBranches === null
            ? commit.remoteBranchRefs
            : commit.remoteBranchRefs.filter((ref) => shownRemoteBranches.has(ref))
          ).map((ref) => (
            <span key={ref} className={styles.remoteBranchBadge}>
              {ref}
            </span>
          ))}
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
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => {
            setContextMenu(null);
            listRef.current?.focus();
          }}
          items={
            multiSelected && effectiveSelection.ids.has(contextMenu.commitId)
              ? multiSelectItems
              : ([
                  {
                    label: "Branch from here",
                    onSelect: () => onBranchFromCommit(contextMenu.commitId),
                  },
                  {
                    label: "Rebase onto here",
                    onSelect: () => onRebaseFromCommit(contextMenu.commitId),
                    disabled: pending,
                  },
                ] satisfies ContextMenuItem[])
          }
        />
      )}
    </ul>
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
