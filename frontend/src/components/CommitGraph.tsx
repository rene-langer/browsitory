import { useMemo, useState, type KeyboardEvent, type MouseEvent } from "react";
import type { GraphCommit, StatusEntry } from "../ipc/RepoClient";
import { assignLanes, isSquashableRange } from "../lib/commitGraphLayout";
import type { SelectedRow } from "../state/useAppState";
import { CommitLaneGraphic } from "./CommitLaneGraphic";
import { ListRow } from "./primitives/ListRow";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import styles from "./CommitGraph.module.css";

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

  const handleKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.key === "ArrowDown" || event.key === "j") {
      event.preventDefault();
      onSelectRow(rows[Math.min(selectedIndex + 1, rows.length - 1)]);
    } else if (event.key === "ArrowUp" || event.key === "k") {
      event.preventDefault();
      onSelectRow(rows[Math.max(selectedIndex - 1, 0)]);
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
  const laneCount =
    Math.max(
      0,
      ...commitLayouts.map((l) => l.lane),
      ...commitLayouts.flatMap((l) => l.passThroughLanes.map((entry) => entry.lane)),
      ...commitLayouts.flatMap((l) => l.parentConnections.map((c) => c.lane)),
    ) + 1;

  return (
    <ul className={styles.list} onKeyDown={handleKeyDown} tabIndex={0} role="listbox" aria-label="Commit history">
      <ListRow
        selected={selectedRow === "uncommitted"}
        onClick={() => {
          setSquashAnchorIndex(null);
          setSquashRange(null);
          onSelectRow("uncommitted");
        }}
      >
        Uncommitted Changes{status.length > 0 && ` (${status.length})`}
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
          <span className={styles.commitSummary}>
            {commit.shortId} {commit.summary}
          </span>
        </ListRow>
      ))}
      {contextMenu !== null && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
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
  );
}
