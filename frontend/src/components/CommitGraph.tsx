import { useMemo, useState, type KeyboardEvent, type MouseEvent } from "react";
import type { GraphCommit, StatusEntry } from "../ipc/RepoClient";
import { assignLanes } from "../lib/commitGraphLayout";
import type { SelectedRow } from "../state/useAppState";
import { CommitLaneGraphic } from "./CommitLaneGraphic";
import { ListRow } from "./primitives/ListRow";
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
}) {
  const [contextMenu, setContextMenu] = useState<{
    commitId: string;
    x: number;
    y: number;
  } | null>(null);

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

  const commitLayouts = useMemo(() => assignLanes(commits), [commits]);
  const laneCount =
    Math.max(
      0,
      ...commitLayouts.map((l) => l.lane),
      ...commitLayouts.flatMap((l) => l.passThroughLanes),
      ...commitLayouts.flatMap((l) => l.parentConnections.map((c) => c.lane)),
    ) + 1;

  return (
    <ul className={styles.list} onKeyDown={handleKeyDown} tabIndex={0} role="listbox" aria-label="Commit history">
      <ListRow selected={selectedRow === "uncommitted"} onClick={() => onSelectRow("uncommitted")}>
        Uncommitted Changes{status.length > 0 && ` (${status.length})`}
      </ListRow>
      {commits.map((commit, index) => (
        <ListRow
          key={commit.id}
          className="commit-row"
          selected={typeof selectedRow === "object" && selectedRow.commitId === commit.id}
          onClick={() => onSelectRow({ commitId: commit.id })}
          onContextMenu={(event) => handleContextMenu(event, commit.id)}
        >
          <CommitLaneGraphic layout={commitLayouts[index]} totalLanes={laneCount} />
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
        <ul
          style={{ position: "fixed", top: contextMenu.y, left: contextMenu.x }}
          onMouseLeave={() => setContextMenu(null)}
        >
          <li>
            <button
              onClick={() => {
                onBranchFromCommit(contextMenu.commitId);
                setContextMenu(null);
              }}
            >
              Branch from here
            </button>
          </li>
          <li>
            <button
              disabled={pending}
              onClick={() => {
                onRebaseFromCommit(contextMenu.commitId);
                setContextMenu(null);
              }}
            >
              Rebase onto here
            </button>
          </li>
        </ul>
      )}
    </ul>
  );
}
