import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import type { GraphCommit, StatusEntry } from "../ipc/RepoClient";
import { assignLanes, isSquashableRange } from "../lib/commitGraphLayout";
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
  onSquashCommits,
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
  // Called when the user squashes a shift-selected range of commits from the graph. `ontoId` is
  // the oldest selected commit's own parent (the base the whole group rebases onto); `squashIds`
  // are the newer selected commits that fold into that oldest one, which survives as the group's
  // leader.
  onSquashCommits?: (ontoId: string, squashIds: string[]) => void;
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
  const [squashAnchorIndex, setSquashAnchorIndex] = useState<number | null>(null);
  const [squashRange, setSquashRange] = useState<{ start: number; end: number } | null>(null);

  const rows: SelectedRow[] = [
    "uncommitted",
    ...commits.map((commit) => ({ commitId: commit.id })),
  ];
  const selectedIndex = rows.findIndex((row) => rowsEqual(row, selectedRow));

  const listRef = useRef<HTMLUListElement>(null);

  // Keep the keyboard-driven selection visible in a long history.
  useEffect(() => {
    const selected = listRef.current?.querySelector<HTMLElement>('[aria-selected="true"]');
    if (typeof selected?.scrollIntoView === "function") selected.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const moveSelection = (nextIndex: number, extendRange: boolean) => {
    // Arrowing/paging past the last loaded row pulls in the next page instead of dead-ending.
    if (nextIndex > rows.length - 1 && hasMore) onLoadMore?.();
    const next = Math.max(0, Math.min(nextIndex, rows.length - 1));
    if (extendRange && next >= 1) {
      // Row 0 is "Uncommitted Changes"; commit indexes are offset by one.
      const anchor = squashAnchorIndex ?? (selectedIndex >= 1 ? selectedIndex - 1 : next - 1);
      setSquashAnchorIndex(anchor);
      setSquashRange({ start: Math.min(anchor, next - 1), end: Math.max(anchor, next - 1) });
    } else {
      setSquashAnchorIndex(next >= 1 ? next - 1 : null);
      setSquashRange(null);
    }
    onSelectRow(rows[next]);
  };

  const openMenuForSelected = (list: HTMLUListElement) => {
    if (selectedIndex < 1) return;
    const row = list.querySelector<HTMLElement>('[aria-selected="true"]');
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
    } else if (event.key === "Enter" || event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) {
      event.preventDefault();
      openMenuForSelected(event.currentTarget);
    }
  };

  const handleContextMenu = (event: MouseEvent, commitId: string) => {
    event.preventDefault();
    setContextMenu({ commitId, x: event.clientX, y: event.clientY });
  };

  const handleCommitClick = (event: MouseEvent | undefined, index: number, commitId: string) => {
    if (event?.shiftKey && squashAnchorIndex !== null) {
      setSquashRange({
        start: Math.min(squashAnchorIndex, index),
        end: Math.max(squashAnchorIndex, index),
      });
    } else {
      setSquashAnchorIndex(index);
      setSquashRange(null);
    }
    onSelectRow({ commitId });
  };

  const activeSquashRange =
    squashRange !== null &&
    squashRange.end > squashRange.start &&
    isSquashableRange(commits, squashRange.start, squashRange.end) &&
    commits[squashRange.end].parentIds.length === 1
      ? squashRange
      : null;

  const contextMenuIndex =
    contextMenu === null ? -1 : commits.findIndex((commit) => commit.id === contextMenu.commitId);
  const squashMenuActive =
    activeSquashRange !== null &&
    contextMenuIndex >= activeSquashRange.start &&
    contextMenuIndex <= activeSquashRange.end;

  const commitLayouts = useMemo(() => assignLanes(commits), [commits]);
  const shownRemoteBranches =
    graphRemoteBranchSelection ?? Array.from(new Set(commits.flatMap((c) => c.remoteBranchRefs)));
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
      aria-label="Commit history"
      aria-keyshortcuts="ArrowUp ArrowDown Home End PageUp PageDown Shift+ArrowUp Shift+ArrowDown Enter ContextMenu Shift+F10"
    >
      <ListRow
        className={styles.uncommittedRow}
        selected={selectedRow === "uncommitted"}
        onClick={() => {
          setSquashAnchorIndex(null);
          setSquashRange(null);
          onSelectRow("uncommitted");
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
          selected={typeof selectedRow === "object" && selectedRow.commitId === commit.id}
          onClick={(event) => handleCommitClick(event, index, commit.id)}
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
          {commit.remoteBranchRefs
            .filter((ref) => shownRemoteBranches.includes(ref))
            .map((ref) => (
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
            squashMenuActive && activeSquashRange !== null
              ? [
                  {
                    label: `Squash ${activeSquashRange.end - activeSquashRange.start + 1} commits`,
                    onSelect: () => {
                      const ontoId = commits[activeSquashRange.end].parentIds[0];
                      const squashIds = commits
                        .slice(activeSquashRange.start, activeSquashRange.end)
                        .map((commit) => commit.id);
                      onSquashCommits?.(ontoId, squashIds);
                    },
                  },
                ]
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
