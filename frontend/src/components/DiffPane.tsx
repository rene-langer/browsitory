import { useEffect, useState } from "react";
import type { DiffHunk, RepoClient, StatusEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { CommitBox } from "./CommitBox";
import { DiffView } from "./DiffView";

export function DiffPane({
  client,
  selectedRow,
  status,
  onStageFile,
  onUnstageFile,
  onCommit,
}: {
  client: RepoClient;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onCommit: (message: string) => void;
}) {
  if (selectedRow === "uncommitted") {
    return (
      <UncommittedDiffPane
        client={client}
        status={status}
        onStageFile={onStageFile}
        onUnstageFile={onUnstageFile}
        onCommit={onCommit}
      />
    );
  }
  return (
    <CommitDiffPane key={selectedRow.commitId} client={client} commitId={selectedRow.commitId} />
  );
}

function UncommittedDiffPane({
  client,
  status,
  onStageFile,
  onUnstageFile,
  onCommit,
}: {
  client: RepoClient;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onCommit: (message: string) => void;
}) {
  const [selected, setSelected] = useState<{ path: string; staged: boolean } | null>(null);
  const [hunks, setHunks] = useState<DiffHunk[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (selected === null) {
      return;
    }
    client
      .getWorkingDiff(selected.path, selected.staged)
      .then((next) => {
        setHunks(next);
        setError(null);
      })
      .catch((err: unknown) => setError(String(err)));
  }, [client, selected]);

  const stagedCount = status.filter((entry) => entry.staged).length;
  const displayedHunks = selected === null ? [] : hunks;

  return (
    <div>
      <ul>
        {status.map((entry) => (
          <li key={`${entry.staged}:${entry.path}`}>
            <button onClick={() => setSelected({ path: entry.path, staged: entry.staged })}>
              {entry.path} ({entry.kind})
            </button>
            {entry.staged ? (
              <button onClick={() => onUnstageFile(entry.path)}>Unstage</button>
            ) : (
              <button onClick={() => onStageFile(entry.path)}>Stage</button>
            )}
          </li>
        ))}
      </ul>
      {error !== null ? <p role="alert">{error}</p> : <DiffView hunks={displayedHunks} />}
      <CommitBox onCommit={onCommit} disabled={stagedCount === 0} />
    </div>
  );
}

function CommitDiffPane({ client, commitId }: { client: RepoClient; commitId: string }) {
  const [files, setFiles] = useState<string[]>([]);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [hunks, setHunks] = useState<DiffHunk[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    client
      .getCommitFiles(commitId)
      .then((next) => {
        setFiles(next);
        setError(null);
      })
      .catch((err: unknown) => setError(String(err)));
  }, [client, commitId]);

  useEffect(() => {
    if (selectedPath === null) {
      return;
    }
    client
      .getCommitDiff(commitId, selectedPath)
      .then((next) => {
        setHunks(next);
        setError(null);
      })
      .catch((err: unknown) => setError(String(err)));
  }, [client, commitId, selectedPath]);

  const displayedHunks = selectedPath === null ? [] : hunks;

  return (
    <div>
      <ul>
        {files.map((path) => (
          <li key={path}>
            <button onClick={() => setSelectedPath(path)}>{path}</button>
          </li>
        ))}
      </ul>
      {error !== null ? <p role="alert">{error}</p> : <DiffView hunks={displayedHunks} />}
    </div>
  );
}
