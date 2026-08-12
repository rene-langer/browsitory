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
  onSaveStash,
}: {
  client: RepoClient;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onCommit: (message: string) => void;
  onSaveStash: () => void;
}) {
  if (selectedRow === "uncommitted") {
    return (
      <UncommittedDiffPane
        client={client}
        status={status}
        onStageFile={onStageFile}
        onUnstageFile={onUnstageFile}
        onCommit={onCommit}
        onSaveStash={onSaveStash}
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
  onSaveStash,
}: {
  client: RepoClient;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onCommit: (message: string) => void;
  onSaveStash: () => void;
}) {
  const [selected, setSelected] = useState<{ path: string; staged: boolean } | null>(null);
  const [hunks, setHunks] = useState<DiffHunk[]>([]);
  const [error, setError] = useState<string | null>(null);

  // `status` is a dependency even though it isn't read here: staging, unstaging or committing
  // the file currently on screen refreshes `status` without changing `selected` by reference,
  // and the displayed diff is stale afterwards (an "unstaged" diff for a file that is now
  // staged, or a working-tree diff for a file that was just committed away). Re-fetching on
  // every `status` change is what keeps the pane honest.
  //
  // `ignore` closes the companion race: rapid clicking between files leaves several fetches in
  // flight, and a slow earlier one resolving after a fast later one would clobber the correct
  // diff. Anything whose effect has already been cleaned up is discarded.
  useEffect(() => {
    if (selected === null) {
      return;
    }
    let ignore = false;
    client
      .getWorkingDiff(selected.path, selected.staged)
      .then((next) => {
        if (!ignore) {
          setHunks(next);
          setError(null);
        }
      })
      .catch((err: unknown) => {
        if (!ignore) {
          setError(String(err));
        }
      });
    return () => {
      ignore = true;
    };
  }, [client, selected, status]);

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
      <button onClick={onSaveStash}>Stash</button>
      <CommitBox onCommit={onCommit} disabled={stagedCount === 0} />
    </div>
  );
}

function CommitDiffPane({ client, commitId }: { client: RepoClient; commitId: string }) {
  const [files, setFiles] = useState<string[]>([]);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [hunks, setHunks] = useState<DiffHunk[]>([]);
  const [error, setError] = useState<string | null>(null);

  // Same `ignore` guard as `UncommittedDiffPane`: `commitId` changes remount this component
  // (the `key` in `DiffPane`), but rapid file switching *within* one mount can still land a
  // slow fetch after a fast one.
  useEffect(() => {
    let ignore = false;
    client
      .getCommitFiles(commitId)
      .then((next) => {
        if (!ignore) {
          setFiles(next);
          setError(null);
        }
      })
      .catch((err: unknown) => {
        if (!ignore) {
          setError(String(err));
        }
      });
    return () => {
      ignore = true;
    };
  }, [client, commitId]);

  useEffect(() => {
    if (selectedPath === null) {
      return;
    }
    let ignore = false;
    client
      .getCommitDiff(commitId, selectedPath)
      .then((next) => {
        if (!ignore) {
          setHunks(next);
          setError(null);
        }
      })
      .catch((err: unknown) => {
        if (!ignore) {
          setError(String(err));
        }
      });
    return () => {
      ignore = true;
    };
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
