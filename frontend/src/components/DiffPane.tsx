import { AlertTriangle, ArrowRightLeft, FileDiff, FileMinus, FilePlus, Pencil, type LucideIcon } from "lucide-react";
import { useEffect, useState, type KeyboardEvent } from "react";
import type {
  BlameLine,
  DiffHunk,
  FileConflictChoice,
  RepoClient,
  StatusEntry,
  StatusKind,
} from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { BlameView } from "./BlameView";
import { CommitBox } from "./CommitBox";
import { ConflictResolutionPane } from "./ConflictResolutionPane";
import { DiffView } from "./DiffView";
import styles from "./DiffPane.module.css";
import { ListRow } from "./primitives/ListRow";
import { RebaseProgressPanel } from "./RebaseProgressPanel";

const STATUS_ICONS: Record<StatusKind, LucideIcon> = {
  New: FilePlus,
  Modified: Pencil,
  Deleted: FileMinus,
  Renamed: ArrowRightLeft,
  TypeChange: FileDiff,
  Conflicted: AlertTriangle,
};

function FileListRow({
  entry,
  selected,
  onSelect,
  onBlame,
  onStageFile,
  onUnstageFile,
}: {
  entry: StatusEntry;
  selected: boolean;
  onSelect: () => void;
  onBlame: () => void;
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
}) {
  const Icon = STATUS_ICONS[entry.kind];
  return (
    <ListRow selected={selected} onClick={onSelect} className={styles.fileRow}>
      <Icon size={14} className={styles.statusIcon} aria-hidden="true" />
      <span className={styles.path}>
        {entry.path} ({entry.kind})
      </span>
      <div className={styles.rowActions}>
        <button
          type="button"
          onClick={(event) => {
            event.stopPropagation();
            onBlame();
          }}
        >
          Blame
        </button>
        {entry.staged ? (
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onUnstageFile(entry.path);
            }}
          >
            Unstage
          </button>
        ) : (
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onStageFile(entry.path);
            }}
          >
            Stage
          </button>
        )}
      </div>
    </ListRow>
  );
}

export function DiffPane({
  repoPath,
  client,
  selectedRow,
  status,
  onStageFile,
  onUnstageFile,
  onStageHunk,
  onUnstageHunk,
  onDiscardHunk,
  onCommit,
  onSaveStash,
  onSelectRow,
  onResolveConflict,
  onResolveAddDeleteConflict,
  mergeMessage,
  onAbortMerge,
  rebaseProgress,
  onRebaseContinue,
  onRebaseAbort,
}: {
  repoPath: string;
  client: RepoClient;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onStageHunk: (path: string, oldStart: number, newStart: number) => void;
  onUnstageHunk: (path: string, oldStart: number, newStart: number) => void;
  onDiscardHunk: (path: string, oldStart: number, newStart: number) => void;
  onCommit: (message: string) => void;
  onSaveStash: () => void;
  onSelectRow: (row: SelectedRow) => void;
  onResolveConflict: (path: string, resolvedContent: string) => void;
  onResolveAddDeleteConflict: (path: string, choice: FileConflictChoice) => void;
  mergeMessage: string | null;
  onAbortMerge: () => void;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  onRebaseContinue: () => void;
  onRebaseAbort: () => void;
}) {
  if (selectedRow === "uncommitted") {
    return (
      <UncommittedDiffPane
        repoPath={repoPath}
        client={client}
        status={status}
        onStageFile={onStageFile}
        onUnstageFile={onUnstageFile}
        onStageHunk={onStageHunk}
        onUnstageHunk={onUnstageHunk}
        onDiscardHunk={onDiscardHunk}
        onCommit={onCommit}
        onSaveStash={onSaveStash}
        onSelectRow={onSelectRow}
        onResolveConflict={onResolveConflict}
        onResolveAddDeleteConflict={onResolveAddDeleteConflict}
        mergeMessage={mergeMessage}
        onAbortMerge={onAbortMerge}
        rebaseProgress={rebaseProgress}
        onRebaseContinue={onRebaseContinue}
        onRebaseAbort={onRebaseAbort}
      />
    );
  }
  return (
    <CommitDiffPane
      key={selectedRow.commitId}
      repoPath={repoPath}
      client={client}
      commitId={selectedRow.commitId}
      onSelectRow={onSelectRow}
    />
  );
}

function UncommittedDiffPane({
  repoPath,
  client,
  status,
  onStageFile,
  onUnstageFile,
  onStageHunk,
  onUnstageHunk,
  onDiscardHunk,
  onCommit,
  onSaveStash,
  onSelectRow,
  onResolveConflict,
  onResolveAddDeleteConflict,
  mergeMessage,
  onAbortMerge,
  rebaseProgress,
  onRebaseContinue,
  onRebaseAbort,
}: {
  repoPath: string;
  client: RepoClient;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onStageHunk: (path: string, oldStart: number, newStart: number) => void;
  onUnstageHunk: (path: string, oldStart: number, newStart: number) => void;
  onDiscardHunk: (path: string, oldStart: number, newStart: number) => void;
  onCommit: (message: string) => void;
  onSaveStash: () => void;
  onSelectRow: (row: SelectedRow) => void;
  onResolveConflict: (path: string, resolvedContent: string) => void;
  onResolveAddDeleteConflict: (path: string, choice: FileConflictChoice) => void;
  mergeMessage: string | null;
  onAbortMerge: () => void;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  onRebaseContinue: () => void;
  onRebaseAbort: () => void;
}) {
  const [selected, setSelected] = useState<{ path: string; staged: boolean } | null>(null);
  const [viewMode, setViewMode] = useState<"diff" | "blame" | "conflict">("diff");
  const [hunks, setHunks] = useState<DiffHunk[]>([]);
  const [blameLines, setBlameLines] = useState<BlameLine[]>([]);
  const [error, setError] = useState<string | null>(null);

  // `status` is a dependency even though it isn't read here: staging, unstaging or committing
  // the file currently on screen refreshes `status` without changing `selected` by reference,
  // and the displayed diff is stale afterwards (an "unstaged" diff for a file that is now
  // staged, or a working-tree diff for a file that was just committed away). Re-fetching on
  // every `status` change is what keeps the pane honest. The blame effect below needs the same
  // `status` dependency for the same reason: staging/committing the file while blame view is
  // open must not leave stale pre-commit attribution on screen.
  //
  // `ignore` closes the companion race: rapid clicking between files leaves several fetches in
  // flight, and a slow earlier one resolving after a fast later one would clobber the correct
  // diff. Anything whose effect has already been cleaned up is discarded.
  useEffect(() => {
    if (selected === null || viewMode !== "diff") {
      return;
    }
    let ignore = false;
    client
      .getWorkingDiff(repoPath, selected.path, selected.staged)
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
  }, [repoPath, client, selected, status, viewMode]);

  // Same `ignore` guard, gated on `viewMode === "blame"` instead. Blame always targets `"HEAD"`
  // — blaming a dirty working-tree edit isn't meaningful (see the design spec's non-goals).
  // `status` is a dependency for the same reason it's on the diff effect above: staging or
  // committing the file on screen while blame view is still open must trigger a refetch, or
  // the pane keeps showing pre-commit attribution indefinitely.
  useEffect(() => {
    if (selected === null || viewMode !== "blame") {
      return;
    }
    let ignore = false;
    client
      .getBlame(repoPath, "HEAD", selected.path)
      .then((next) => {
        if (!ignore) {
          setBlameLines(next);
          setError(null);
        }
      })
      .catch(() => {
        if (!ignore) {
          // libgit2's rejection message (e.g. "the path 'x' does not exist in the given tree")
          // is jarring for what's usually just an unexceptional case — blaming a new/untracked
          // file that isn't in HEAD's tree yet.
          setError("No blame available for this file at this revision.");
        }
      });
    return () => {
      ignore = true;
    };
  }, [repoPath, client, selected, viewMode, status]);

  // Once the selected file's conflict is resolved (via this pane, or externally e.g. abort),
  // `status` no longer lists it as `Conflicted`, so the render below correctly stops showing
  // `ConflictResolutionPane` — but `viewMode` itself is still `"conflict"`, and the diff-fetch
  // effect above only runs when `viewMode === "diff"`. Left alone, the pane falls through to
  // `DiffView` with stale/empty `hunks` instead of the real post-resolution diff. Transitioning
  // back to `"diff"` here lets that effect fire and fetch the real diff.
  useEffect(() => {
    if (
      viewMode === "conflict" &&
      selected !== null &&
      !status.some((entry) => entry.path === selected.path && entry.kind === "Conflicted")
    ) {
      // Deliberate view-mode transition, not a synchronization loop — see comment above.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setViewMode("diff");
    }
  }, [viewMode, selected, status]);

  const stagedCount = status.filter((entry) => entry.staged).length;
  const displayedHunks = selected === null || viewMode !== "diff" ? [] : hunks;
  const displayedBlameLines = selected === null || viewMode !== "blame" ? [] : blameLines;

  const stagedEntries = status.filter((entry) => entry.staged);
  const unstagedEntries = status.filter((entry) => !entry.staged);

  const isEntrySelected = (entry: StatusEntry) =>
    selected !== null && selected.path === entry.path && selected.staged === entry.staged;

  const selectEntry = (entry: StatusEntry) => {
    setSelected({ path: entry.path, staged: entry.staged });
    setViewMode(entry.kind === "Conflicted" ? "conflict" : "diff");
  };

  const blameEntry = (entry: StatusEntry) => {
    setSelected({ path: entry.path, staged: entry.staged });
    setViewMode("blame");
  };

  const navigateGroup = (entries: StatusEntry[], direction: 1 | -1) => {
    if (entries.length === 0) return;
    const currentIndex = entries.findIndex(
      (entry) => selected !== null && entry.path === selected.path && entry.staged === selected.staged,
    );
    const nextIndex =
      currentIndex === -1 ? 0 : Math.min(Math.max(currentIndex + direction, 0), entries.length - 1);
    selectEntry(entries[nextIndex]);
  };

  const handleGroupKeyDown = (entries: StatusEntry[]) => (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.key === "ArrowDown" || event.key === "j") {
      event.preventDefault();
      navigateGroup(entries, 1);
    } else if (event.key === "ArrowUp" || event.key === "k") {
      event.preventDefault();
      navigateGroup(entries, -1);
    }
  };

  return (
    <div>
      {unstagedEntries.length > 0 && (
        <div>
          <div className={styles.groupHeading}>
            <span>Changes ({unstagedEntries.length})</span>
          </div>
          <ul
            className={styles.fileList}
            role="listbox"
            aria-label="Unstaged changes"
            tabIndex={0}
            onKeyDown={handleGroupKeyDown(unstagedEntries)}
          >
            {unstagedEntries.map((entry) => (
              <FileListRow
                key={`${entry.staged}:${entry.path}`}
                entry={entry}
                selected={isEntrySelected(entry)}
                onSelect={() => selectEntry(entry)}
                onBlame={() => blameEntry(entry)}
                onStageFile={onStageFile}
                onUnstageFile={onUnstageFile}
              />
            ))}
          </ul>
        </div>
      )}
      {stagedEntries.length > 0 && (
        <div>
          <div className={styles.groupHeading}>
            <span>Staged ({stagedEntries.length})</span>
          </div>
          <ul
            className={styles.fileList}
            role="listbox"
            aria-label="Staged changes"
            tabIndex={0}
            onKeyDown={handleGroupKeyDown(stagedEntries)}
          >
            {stagedEntries.map((entry) => (
              <FileListRow
                key={`${entry.staged}:${entry.path}`}
                entry={entry}
                selected={isEntrySelected(entry)}
                onSelect={() => selectEntry(entry)}
                onBlame={() => blameEntry(entry)}
                onStageFile={onStageFile}
                onUnstageFile={onUnstageFile}
              />
            ))}
          </ul>
        </div>
      )}
      {viewMode === "blame" ? (
        <>
          {error !== null ? (
            <p role="alert">{error}</p>
          ) : (
            <BlameView lines={displayedBlameLines} onSelectRow={onSelectRow} />
          )}
          <button onClick={() => setViewMode("diff")}>Back to Diff</button>
        </>
      ) : viewMode === "conflict" &&
        selected !== null &&
        status.some((entry) => entry.path === selected.path && entry.kind === "Conflicted") ? (
        <ConflictResolutionPane
          key={selected.path}
          repoPath={repoPath}
          client={client}
          path={selected.path}
          onResolve={onResolveConflict}
          onResolveAddDelete={onResolveAddDeleteConflict}
        />
      ) : error !== null ? (
        <p role="alert">{error}</p>
      ) : (
        <DiffView
          hunks={displayedHunks}
          onStageHunk={
            selected !== null && !selected.staged
              ? (hunkOldStart: number, hunkNewStart: number) => onStageHunk(selected.path, hunkOldStart, hunkNewStart)
              : undefined
          }
          onUnstageHunk={
            selected !== null && selected.staged
              ? (hunkOldStart: number, hunkNewStart: number) => onUnstageHunk(selected.path, hunkOldStart, hunkNewStart)
              : undefined
          }
          onDiscardHunk={
            selected !== null
              ? (hunkOldStart: number, hunkNewStart: number) => onDiscardHunk(selected.path, hunkOldStart, hunkNewStart)
              : undefined
          }
        />
      )}
      {/* Stashing mid-rebase is destructive in a way nothing else undoes: a paused step's
          resolved/amended content lives in the working tree, so stashing it away and continuing
          lands an empty (or wrong) commit. Disabled for the whole pause, same rule as
          `BranchSwitcher`'s ref-mutating actions. */}
      <button onClick={onSaveStash} disabled={status.length === 0 || rebaseProgress !== null}>
        Stash
      </button>
      {rebaseProgress !== null ? (
        <RebaseProgressPanel
          currentStep={rebaseProgress.currentStep}
          totalSteps={rebaseProgress.totalSteps}
          disabled={status.some((entry) => entry.kind === "Conflicted")}
          onContinue={onRebaseContinue}
          onAbort={onRebaseAbort}
        />
      ) : (
        <CommitBox
          onCommit={onCommit}
          disabled={stagedCount === 0 || status.some((entry) => entry.kind === "Conflicted")}
          onAbortMerge={onAbortMerge}
          initialMessage={mergeMessage ?? undefined}
        />
      )}
    </div>
  );
}

function CommitDiffPane({
  repoPath,
  client,
  commitId,
  onSelectRow,
}: {
  repoPath: string;
  client: RepoClient;
  commitId: string;
  onSelectRow: (row: SelectedRow) => void;
}) {
  const [files, setFiles] = useState<string[]>([]);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"diff" | "blame">("diff");
  const [hunks, setHunks] = useState<DiffHunk[]>([]);
  const [blameLines, setBlameLines] = useState<BlameLine[]>([]);
  const [error, setError] = useState<string | null>(null);

  // Same `ignore` guard as `UncommittedDiffPane`: `commitId` changes remount this component
  // (the `key` in `DiffPane`), but rapid file switching *within* one mount can still land a
  // slow fetch after a fast one.
  useEffect(() => {
    let ignore = false;
    client
      .getCommitFiles(repoPath, commitId)
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
  }, [repoPath, client, commitId]);

  useEffect(() => {
    if (selectedPath === null || viewMode !== "diff") {
      return;
    }
    let ignore = false;
    client
      .getCommitDiff(repoPath, commitId, selectedPath)
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
  }, [repoPath, client, commitId, selectedPath, viewMode]);

  useEffect(() => {
    if (selectedPath === null || viewMode !== "blame") {
      return;
    }
    let ignore = false;
    client
      .getBlame(repoPath, commitId, selectedPath)
      .then((next) => {
        if (!ignore) {
          setBlameLines(next);
          setError(null);
        }
      })
      .catch(() => {
        if (!ignore) {
          // See UncommittedDiffPane's blame effect: swap libgit2's raw rejection message for a
          // friendlier one — this is a common, unexceptional case (e.g. a file that was added
          // or removed relative to this commit's parent).
          setError("No blame available for this file at this revision.");
        }
      });
    return () => {
      ignore = true;
    };
  }, [repoPath, client, commitId, selectedPath, viewMode]);

  const displayedHunks = selectedPath === null || viewMode !== "diff" ? [] : hunks;
  const displayedBlameLines = selectedPath === null || viewMode !== "blame" ? [] : blameLines;

  return (
    <div>
      <ul className={styles.fileList}>
        {files.map((path) => (
          <li key={path}>
            <button
              onClick={() => {
                setSelectedPath(path);
                setViewMode("diff");
              }}
            >
              {path}
            </button>
            <button
              onClick={() => {
                setSelectedPath(path);
                setViewMode("blame");
              }}
            >
              Blame
            </button>
          </li>
        ))}
      </ul>
      {viewMode === "blame" ? (
        <>
          {error !== null ? (
            <p role="alert">{error}</p>
          ) : (
            <BlameView lines={displayedBlameLines} onSelectRow={onSelectRow} />
          )}
          <button onClick={() => setViewMode("diff")}>Back to Diff</button>
        </>
      ) : error !== null ? (
        <p role="alert">{error}</p>
      ) : (
        <DiffView hunks={displayedHunks} />
      )}
    </div>
  );
}
