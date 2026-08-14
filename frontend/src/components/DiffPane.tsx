import { useEffect, useState } from "react";
import type {
  BlameLine,
  DiffHunk,
  FileConflictChoice,
  RepoClient,
  StatusEntry,
} from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { BlameView } from "./BlameView";
import { CommitBox } from "./CommitBox";
import { ConflictResolutionPane } from "./ConflictResolutionPane";
import { DiffView } from "./DiffView";
import { RebaseProgressPanel } from "./RebaseProgressPanel";

export function DiffPane({
  client,
  selectedRow,
  status,
  onStageFile,
  onUnstageFile,
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
  client: RepoClient;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
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
        client={client}
        status={status}
        onStageFile={onStageFile}
        onUnstageFile={onUnstageFile}
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
      client={client}
      commitId={selectedRow.commitId}
      onSelectRow={onSelectRow}
    />
  );
}

function UncommittedDiffPane({
  client,
  status,
  onStageFile,
  onUnstageFile,
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
  client: RepoClient;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
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
  }, [client, selected, status, viewMode]);

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
      .getBlame("HEAD", selected.path)
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
  }, [client, selected, viewMode, status]);

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

  return (
    <div>
      <ul>
        {status.map((entry) => (
          <li key={`${entry.staged}:${entry.path}`}>
            <button
              onClick={() => {
                setSelected({ path: entry.path, staged: entry.staged });
                setViewMode(entry.kind === "Conflicted" ? "conflict" : "diff");
              }}
            >
              {entry.path} ({entry.kind})
            </button>
            <button
              onClick={() => {
                setSelected({ path: entry.path, staged: entry.staged });
                setViewMode("blame");
              }}
            >
              Blame
            </button>
            {entry.staged ? (
              <button onClick={() => onUnstageFile(entry.path)}>Unstage</button>
            ) : (
              <button onClick={() => onStageFile(entry.path)}>Stage</button>
            )}
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
      ) : viewMode === "conflict" &&
        selected !== null &&
        status.some((entry) => entry.path === selected.path && entry.kind === "Conflicted") ? (
        <ConflictResolutionPane
          key={selected.path}
          client={client}
          path={selected.path}
          onResolve={onResolveConflict}
          onResolveAddDelete={onResolveAddDeleteConflict}
        />
      ) : error !== null ? (
        <p role="alert">{error}</p>
      ) : (
        <DiffView hunks={displayedHunks} />
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
  client,
  commitId,
  onSelectRow,
}: {
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
    if (selectedPath === null || viewMode !== "diff") {
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
  }, [client, commitId, selectedPath, viewMode]);

  useEffect(() => {
    if (selectedPath === null || viewMode !== "blame") {
      return;
    }
    let ignore = false;
    client
      .getBlame(commitId, selectedPath)
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
  }, [client, commitId, selectedPath, viewMode]);

  const displayedHunks = selectedPath === null || viewMode !== "diff" ? [] : hunks;
  const displayedBlameLines = selectedPath === null || viewMode !== "blame" ? [] : blameLines;

  return (
    <div>
      <ul>
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
