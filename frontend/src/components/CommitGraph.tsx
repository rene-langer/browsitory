import { useMemo, useState, type KeyboardEvent, type MouseEvent } from "react";
import type { GraphCommit, StashEntry, StatusEntry } from "../ipc/RepoClient";
import { assignLanes } from "../lib/commitGraphLayout";
import type { SelectedRow } from "../state/useAppState";
import { CommitLaneGraphic } from "./CommitLaneGraphic";

function rowsEqual(a: SelectedRow, b: SelectedRow): boolean {
  if (a === "uncommitted" || b === "uncommitted") {
    return a === b;
  }
  return a.commitId === b.commitId;
}

export function CommitGraph({
  status,
  commits,
  stashes,
  selectedRow,
  pending,
  onSelectRow,
  onBranchFromCommit,
  onRebaseFromCommit,
  onApplyStash,
  onDropStash,
}: {
  status: StatusEntry[];
  commits: GraphCommit[];
  stashes: StashEntry[];
  selectedRow: SelectedRow;
  // True while a mutation (e.g. an Apply/Drop stash) is in flight. Disables the Apply/Drop
  // buttons below so a rapid double-click can't fire the same index-based mutation twice
  // before the first one's refresh has landed — see `useAppState.ts`'s `runMutation`.
  pending: boolean;
  onSelectRow: (row: SelectedRow) => void;
  onBranchFromCommit: (commitId: string) => void;
  onRebaseFromCommit: (commitId: string) => void;
  onApplyStash: (index: number) => void;
  onDropStash: (index: number) => void;
}) {
  const [contextMenu, setContextMenu] = useState<{
    commitId: string;
    x: number;
    y: number;
  } | null>(null);

  const rows: SelectedRow[] = [
    "uncommitted",
    ...stashes.map((stash) => ({ commitId: stash.commitId })),
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
    <ul onKeyDown={handleKeyDown} tabIndex={0}>
      <li
        aria-selected={selectedRow === "uncommitted"}
        onClick={() => onSelectRow("uncommitted")}
      >
        Uncommitted Changes{status.length > 0 && ` (${status.length})`}
      </li>
      {stashes.map((stash) => (
        <li
          key={stash.commitId}
          className="stash-row"
          aria-selected={
            typeof selectedRow === "object" && selectedRow.commitId === stash.commitId
          }
          onClick={() => onSelectRow({ commitId: stash.commitId })}
        >
          <span>{stash.message}</span>
          <button
            disabled={pending}
            onClick={(event: MouseEvent<HTMLButtonElement>) => {
              event.stopPropagation();
              onApplyStash(stash.index);
            }}
          >
            Apply
          </button>
          <button
            disabled={pending}
            onClick={(event: MouseEvent<HTMLButtonElement>) => {
              event.stopPropagation();
              onDropStash(stash.index);
            }}
          >
            Drop
          </button>
        </li>
      ))}
      {commits.map((commit, index) => (
        <li
          key={commit.id}
          className="commit-row"
          aria-selected={
            typeof selectedRow === "object" && selectedRow.commitId === commit.id
          }
          onClick={() => onSelectRow({ commitId: commit.id })}
          onContextMenu={(event) => handleContextMenu(event, commit.id)}
        >
          <CommitLaneGraphic layout={commitLayouts[index]} totalLanes={laneCount} />
          {commit.branchRefs.map((ref) => (
            <span key={ref} className="branch-badge">
              {ref}
            </span>
          ))}
          <span className="commit-summary">
            {commit.shortId} {commit.summary}
          </span>
        </li>
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
