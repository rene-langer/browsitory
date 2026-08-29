import {
  AlertTriangle,
  ArrowRightLeft,
  ChevronDown,
  ChevronRight,
  FileDiff,
  FileMinus,
  FilePlus,
  Minus,
  Pencil,
  Plus,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
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
import { InlineError } from "./primitives/InlineError";
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

function CollapseToggle({ collapsed, path, onToggle }: { collapsed: boolean; path: string; onToggle: () => void }) {
  return (
    <button
      type="button"
      className={styles.collapseToggle}
      aria-label={collapsed ? `Expand ${path}` : `Collapse ${path}`}
      onClick={(event) => {
        event.stopPropagation();
        onToggle();
      }}
    >
      {collapsed ? (
        <ChevronRight size={14} aria-hidden="true" />
      ) : (
        <ChevronDown size={14} aria-hidden="true" />
      )}
    </button>
  );
}

function CollapseAllToggle({ allCollapsed, onToggle }: { allCollapsed: boolean; onToggle: () => void }) {
  return (
    <button type="button" onClick={onToggle}>
      {allCollapsed ? "Expand all" : "Collapse all"}
    </button>
  );
}

function UncommittedFileSection({
  repoPath,
  client,
  entry,
  status,
  isCurrent,
  collapsed,
  onToggleCollapse,
  onSelect,
  onStageFile,
  onUnstageFile,
  onStageHunk,
  onUnstageHunk,
  onDiscardHunk,
  onSelectRow,
  onResolveConflict,
  onResolveAddDeleteConflict,
  sectionRef,
}: {
  repoPath: string;
  client: RepoClient;
  entry: StatusEntry;
  status: StatusEntry[];
  isCurrent: boolean;
  collapsed: boolean;
  onToggleCollapse: () => void;
  onSelect: () => void;
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onStageHunk: (path: string, oldStart: number, newStart: number) => void;
  onUnstageHunk: (path: string, oldStart: number, newStart: number) => void;
  onDiscardHunk: (path: string, oldStart: number, newStart: number) => void;
  onSelectRow: (row: SelectedRow) => void;
  onResolveConflict: (path: string, resolvedContent: string) => void;
  onResolveAddDeleteConflict: (path: string, choice: FileConflictChoice) => void;
  sectionRef: (el: HTMLDivElement | null) => void;
}) {
  const [mode, setMode] = useState<"diff" | "blame">("diff");
  const [hunks, setHunks] = useState<DiffHunk[]>([]);
  const [blameLines, setBlameLines] = useState<BlameLine[]>([]);
  const [error, setError] = useState<string | null>(null);
  const isConflicted = entry.kind === "Conflicted";

  // Every file's diff is fetched eagerly (all sections render expanded by default), keyed on the
  // file's own identity rather than a shared "selected" pointer. Whole-file staging/unstaging
  // moves this file to a differently-keyed section and remounts it fresh, but *partial* (hunk)
  // staging leaves it at the same path/staged key while only its hunk count changes underneath —
  // `status` stays a dependency for the same reason the old single-pane version needed it: a new
  // `status` array (by reference) is the only signal that this file's own diff may be stale.
  useEffect(() => {
    if (mode !== "diff" || isConflicted) return;
    let ignore = false;
    client
      .getWorkingDiff(repoPath, entry.path, entry.staged)
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
  }, [repoPath, client, entry.path, entry.staged, isConflicted, mode, status]);

  // `status` is a dependency for the same reason as the diff effect above: staging or committing
  // the file on screen while its blame view is open must not leave stale pre-commit attribution
  // on screen.
  useEffect(() => {
    if (mode !== "blame") return;
    let ignore = false;
    client
      .getBlame(repoPath, "HEAD", entry.path)
      .then((next) => {
        if (!ignore) {
          setBlameLines(next);
          setError(null);
        }
      })
      .catch(() => {
        if (!ignore) {
          setError("No blame available for this file at this revision.");
        }
      });
    return () => {
      ignore = true;
    };
  }, [repoPath, client, entry.path, mode, status]);

  const Icon = STATUS_ICONS[entry.kind];

  return (
    <ListRow selected={isCurrent} onClick={onSelect} className={styles.fileSection}>
      <div className={styles.fileSectionHeader}>
        <CollapseToggle collapsed={collapsed} path={entry.path} onToggle={onToggleCollapse} />
        <Icon size={14} className={styles.statusIcon} aria-hidden="true" />
        <span className={styles.path}>
          {entry.path} ({entry.kind})
        </span>
        <div className={styles.rowActions}>
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              setMode("blame");
            }}
          >
            Blame
          </button>
          {entry.staged ? (
            <button
              type="button"
              className={styles.stageToggle}
              aria-label={`Unstage ${entry.path}`}
              onClick={(event) => {
                event.stopPropagation();
                onUnstageFile(entry.path);
              }}
            >
              <Minus size={14} aria-hidden="true" />
            </button>
          ) : (
            <button
              type="button"
              className={styles.stageToggle}
              aria-label={`Stage ${entry.path}`}
              onClick={(event) => {
                event.stopPropagation();
                onStageFile(entry.path);
              }}
            >
              <Plus size={14} aria-hidden="true" />
            </button>
          )}
        </div>
      </div>
      {!collapsed && (
        <div className={styles.fileSectionBody} ref={sectionRef}>
          {mode === "blame" ? (
            <>
              {error !== null ? (
                <InlineError message={error} onDismiss={() => setError(null)} />
              ) : (
                <BlameView lines={blameLines} onSelectRow={onSelectRow} />
              )}
              <button type="button" onClick={() => setMode("diff")}>
                Back to Diff
              </button>
            </>
          ) : isConflicted ? (
            <ConflictResolutionPane
              repoPath={repoPath}
              client={client}
              path={entry.path}
              onResolve={onResolveConflict}
              onResolveAddDelete={onResolveAddDeleteConflict}
            />
          ) : error !== null ? (
            <InlineError message={error} onDismiss={() => setError(null)} />
          ) : (
            <DiffView
              hunks={hunks}
              onStageHunk={!entry.staged ? (oldStart, newStart) => onStageHunk(entry.path, oldStart, newStart) : undefined}
              onUnstageHunk={entry.staged ? (oldStart, newStart) => onUnstageHunk(entry.path, oldStart, newStart) : undefined}
              onDiscardHunk={(oldStart, newStart) => onDiscardHunk(entry.path, oldStart, newStart)}
            />
          )}
        </div>
      )}
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
  onStageAllFiles,
  onUnstageAllFiles,
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
  onStageAllFiles: (paths: string[]) => void;
  onUnstageAllFiles: (paths: string[]) => void;
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
        onStageAllFiles={onStageAllFiles}
        onUnstageAllFiles={onUnstageAllFiles}
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

function entryKey(entry: { path: string; staged: boolean }): string {
  return `${entry.staged}:${entry.path}`;
}

function UncommittedDiffPane({
  repoPath,
  client,
  status,
  onStageFile,
  onUnstageFile,
  onStageAllFiles,
  onUnstageAllFiles,
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
  onStageAllFiles: (paths: string[]) => void;
  onUnstageAllFiles: (paths: string[]) => void;
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
  const [current, setCurrent] = useState<{ path: string; staged: boolean } | null>(null);
  const [collapsedKeys, setCollapsedKeys] = useState<Set<string>>(new Set());
  const sectionRefs = useRef<Record<string, HTMLDivElement | null>>({});

  useEffect(() => {
    if (current === null) return;
    sectionRefs.current[entryKey(current)]?.scrollIntoView?.({ block: "nearest" });
  }, [current]);

  const stagedEntries = status.filter((entry) => entry.staged);
  const unstagedEntries = status.filter((entry) => !entry.staged);
  const stagedCount = stagedEntries.length;
  // `git-core::status` reports conflicted entries with `staged: false`, so they sit in the
  // "Changes" group — but staging a conflicted path is what *marks the conflict resolved*, with
  // whatever happens to be in the working tree. One "Stage all" click would silently resolve
  // every outstanding conflict, so the bulk action skips them. The per-row Stage control on a
  // conflicted file is left alone: that's a deliberate, one-file-at-a-time action.
  const stageAllPaths = unstagedEntries
    .filter((entry) => entry.kind !== "Conflicted")
    .map((entry) => entry.path);

  const allEntries = [...unstagedEntries, ...stagedEntries];
  const allKeys = allEntries.map(entryKey);
  const allCollapsed = allKeys.length > 0 && allKeys.every((key) => collapsedKeys.has(key));

  const isEntrySelected = (entry: StatusEntry) =>
    current !== null && current.path === entry.path && current.staged === entry.staged;

  const selectEntry = (entry: StatusEntry) => {
    setCurrent({ path: entry.path, staged: entry.staged });
  };

  const toggleCollapse = (key: string) => {
    setCollapsedKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  const toggleCollapseAll = () => {
    setCollapsedKeys(allCollapsed ? new Set() : new Set(allKeys));
  };

  const navigateGroup = (entries: StatusEntry[], direction: 1 | -1) => {
    if (entries.length === 0) return;
    const currentIndex = entries.findIndex(
      (entry) => current !== null && entry.path === current.path && entry.staged === current.staged,
    );
    const nextIndex =
      currentIndex === -1 ? 0 : Math.min(Math.max(currentIndex + direction, 0), entries.length - 1);
    selectEntry(entries[nextIndex]);
  };

  // The per-row Stage/Unstage controls are `<button>`s nested inside a `role="option"` row. That
  // keeps them reachable by mouse and by plain Tab, but ARIA's listbox/option pattern treats an
  // option's children as content contributing to the option's name, not as independent widgets —
  // so assistive tech arrowing through this listbox does not reliably surface them. `s` is the
  // keyboard equivalent, on the container that already owns navigation (`j`/`k`/arrows), so the
  // whole stage/unstage flow is doable without ever reaching those buttons. See
  // `primitives/ListRow.tsx`'s doc comment for why the buttons weren't moved out of the row.
  const toggleStageSelected = (entries: StatusEntry[]) => {
    const entry = entries.find(
      (candidate) => current !== null && candidate.path === current.path && candidate.staged === current.staged,
    );
    if (entry === undefined) return;
    if (entry.staged) {
      onUnstageFile(entry.path);
    } else {
      onStageFile(entry.path);
    }
  };

  const handleGroupKeyDown = (entries: StatusEntry[]) => (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.key === "ArrowDown" || event.key === "j") {
      event.preventDefault();
      navigateGroup(entries, 1);
    } else if (event.key === "ArrowUp" || event.key === "k") {
      event.preventDefault();
      navigateGroup(entries, -1);
    } else if (event.key === "s") {
      event.preventDefault();
      toggleStageSelected(entries);
    }
  };

  const handleStageAll = () => {
    onStageAllFiles(stageAllPaths);
  };

  const handleUnstageAll = () => {
    onUnstageAllFiles(stagedEntries.map((entry) => entry.path));
  };

  const renderSection = (entry: StatusEntry) => {
    const key = entryKey(entry);
    return (
      <UncommittedFileSection
        key={key}
        repoPath={repoPath}
        client={client}
        entry={entry}
        status={status}
        isCurrent={isEntrySelected(entry)}
        collapsed={collapsedKeys.has(key)}
        onToggleCollapse={() => toggleCollapse(key)}
        onSelect={() => selectEntry(entry)}
        onStageFile={onStageFile}
        onUnstageFile={onUnstageFile}
        onStageHunk={onStageHunk}
        onUnstageHunk={onUnstageHunk}
        onDiscardHunk={onDiscardHunk}
        onSelectRow={onSelectRow}
        onResolveConflict={onResolveConflict}
        onResolveAddDeleteConflict={onResolveAddDeleteConflict}
        sectionRef={(el) => {
          sectionRefs.current[key] = el;
        }}
      />
    );
  };

  return (
    <div>
      {allKeys.length > 0 && (
        <div className={styles.groupHeading}>
          <CollapseAllToggle allCollapsed={allCollapsed} onToggle={toggleCollapseAll} />
        </div>
      )}
      {unstagedEntries.length > 0 && (
        <div>
          <div className={styles.groupHeading}>
            <span>Changes ({unstagedEntries.length})</span>
            {/* Disabled when every unstaged entry is a conflict — the action would be a no-op
                that still costs a full refresh. */}
            <button type="button" onClick={handleStageAll} disabled={stageAllPaths.length === 0}>
              Stage all
            </button>
          </div>
          <ul
            className={styles.fileList}
            role="listbox"
            aria-label="Unstaged changes"
            aria-keyshortcuts="s"
            tabIndex={0}
            onKeyDown={handleGroupKeyDown(unstagedEntries)}
          >
            {unstagedEntries.map(renderSection)}
          </ul>
        </div>
      )}
      {stagedEntries.length > 0 && (
        <div>
          <div className={styles.groupHeading}>
            <span>Staged ({stagedEntries.length})</span>
            <button type="button" onClick={handleUnstageAll}>
              Unstage all
            </button>
          </div>
          <ul
            className={styles.fileList}
            role="listbox"
            aria-label="Staged changes"
            aria-keyshortcuts="s"
            tabIndex={0}
            onKeyDown={handleGroupKeyDown(stagedEntries)}
          >
            {stagedEntries.map(renderSection)}
          </ul>
        </div>
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

function CommitFileSection({
  repoPath,
  client,
  commitId,
  path,
  collapsed,
  onToggleCollapse,
  onSelectRow,
}: {
  repoPath: string;
  client: RepoClient;
  commitId: string;
  path: string;
  collapsed: boolean;
  onToggleCollapse: () => void;
  onSelectRow: (row: SelectedRow) => void;
}) {
  const [mode, setMode] = useState<"diff" | "blame">("diff");
  const [hunks, setHunks] = useState<DiffHunk[]>([]);
  const [blameLines, setBlameLines] = useState<BlameLine[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (mode !== "diff") return;
    let ignore = false;
    client
      .getCommitDiff(repoPath, commitId, path)
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
  }, [repoPath, client, commitId, path, mode]);

  useEffect(() => {
    if (mode !== "blame") return;
    let ignore = false;
    client
      .getBlame(repoPath, commitId, path)
      .then((next) => {
        if (!ignore) {
          setBlameLines(next);
          setError(null);
        }
      })
      .catch(() => {
        if (!ignore) {
          setError("No blame available for this file at this revision.");
        }
      });
    return () => {
      ignore = true;
    };
  }, [repoPath, client, commitId, path, mode]);

  return (
    <li className={styles.fileSection}>
      <div className={styles.fileSectionHeader}>
        <CollapseToggle collapsed={collapsed} path={path} onToggle={onToggleCollapse} />
        <span className={styles.path}>{path}</span>
        <div className={styles.rowActions}>
          <button type="button" onClick={() => setMode("blame")}>
            Blame
          </button>
        </div>
      </div>
      {!collapsed && (
        <div className={styles.fileSectionBody}>
          {mode === "blame" ? (
            <>
              {error !== null ? (
                <InlineError message={error} onDismiss={() => setError(null)} />
              ) : (
                <BlameView lines={blameLines} onSelectRow={onSelectRow} />
              )}
              <button onClick={() => setMode("diff")}>Back to Diff</button>
            </>
          ) : error !== null ? (
            <InlineError message={error} onDismiss={() => setError(null)} />
          ) : (
            <DiffView hunks={hunks} />
          )}
        </div>
      )}
    </li>
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
  const [error, setError] = useState<string | null>(null);
  const [collapsedPaths, setCollapsedPaths] = useState<Set<string>>(new Set());

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

  const allCollapsed = files.length > 0 && files.every((path) => collapsedPaths.has(path));

  const toggleCollapse = (path: string) => {
    setCollapsedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  const toggleCollapseAll = () => {
    setCollapsedPaths(allCollapsed ? new Set() : new Set(files));
  };

  if (error !== null) {
    return <InlineError message={error} onDismiss={() => setError(null)} />;
  }

  return (
    <div>
      {files.length > 0 && (
        <div className={styles.groupHeading}>
          <CollapseAllToggle allCollapsed={allCollapsed} onToggle={toggleCollapseAll} />
        </div>
      )}
      <ul className={styles.fileList}>
        {files.map((path) => (
          <CommitFileSection
            key={path}
            repoPath={repoPath}
            client={client}
            commitId={commitId}
            path={path}
            collapsed={collapsedPaths.has(path)}
            onToggleCollapse={() => toggleCollapse(path)}
            onSelectRow={onSelectRow}
          />
        ))}
      </ul>
    </div>
  );
}
