import type { KeyboardEvent } from "react";
import type { CommitInfo, StatusEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";

function rowsEqual(a: SelectedRow, b: SelectedRow): boolean {
  if (a === "uncommitted" || b === "uncommitted") {
    return a === b;
  }
  return a.commitId === b.commitId;
}

export function HistoryList({
  status,
  log,
  selectedRow,
  onSelectRow,
}: {
  status: StatusEntry[];
  log: CommitInfo[];
  selectedRow: SelectedRow;
  onSelectRow: (row: SelectedRow) => void;
}) {
  const rows: SelectedRow[] = [
    "uncommitted",
    ...log.map((commit) => ({ commitId: commit.id })),
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

  return (
    <ul onKeyDown={handleKeyDown} tabIndex={0}>
      <li
        aria-selected={selectedRow === "uncommitted"}
        onClick={() => onSelectRow("uncommitted")}
      >
        Uncommitted Changes{status.length > 0 && ` (${status.length})`}
      </li>
      {log.map((commit) => (
        <li
          key={commit.id}
          aria-selected={
            typeof selectedRow === "object" && selectedRow.commitId === commit.id
          }
          onClick={() => onSelectRow({ commitId: commit.id })}
        >
          {commit.shortId} {commit.summary}
        </li>
      ))}
    </ul>
  );
}
