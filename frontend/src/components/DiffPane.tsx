import { describeError, type DescribedError } from "../lib/errorMessages";
import { conflictReason } from "../lib/operationStatus";
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
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import type {
  BlameLine,
  DiffHunk,
  FileConflictChoice,
  GraphCommit,
  RepoClient,
  StatusEntry,
  StatusKind,
} from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { BlameView } from "./BlameView";
import { CommitBox, type CommitDraft } from "./CommitBox";
import { CommitHeader } from "./CommitHeader";
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
  diffVersion,
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
  // This section's staleness signal: its own per-path version plus the pane-wide refresh
  // generation, combined by the parent (`UncommittedDiffPane`) into one monotonically increasing
  // number — see `staleness` there for how each part is bumped. Any change means "this diff may
  // be stale, refetch it".
  diffVersion: number;
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
  const [hunks, setHunks] = useState<DiffHunk[] | null>(null);
  const [blameLines, setBlameLines] = useState<BlameLine[]>([]);
  const [error, setError] = useState<DescribedError | null>(null);
  const isConflicted = entry.kind === "Conflicted";
  // Tracks proximity to the viewport so a long file list doesn't eagerly fetch every section's
  // diff at once (PERF-001) — only sections within `rootMargin` of the viewport ever load.
  const sectionElRef = useRef<HTMLDivElement | null>(null);
  const [nearViewport, setNearViewport] = useState(false);
  useEffect(() => {
    const el = sectionElRef.current;
    if (el === null) return;
    const observer = new IntersectionObserver(([observerEntry]) => setNearViewport(observerEntry.isIntersecting), {
      rootMargin: "200px",
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Every file's diff is fetched keyed on the file's own identity rather than a shared
  // "selected" pointer. Whole-file staging/unstaging moves this file to a differently-keyed
  // section and remounts it fresh, but *partial* (hunk) staging leaves it at the same
  // path/staged key while only its hunk content changes underneath — and so does anything else
  // that changes the working tree or index without moving the file between groups (a stash
  // apply, a pull, the palette's Refresh after an external edit). `diffVersion` is the signal for
  // all of those: the parent bumps it for both sides of a hunk-mutated path once the mutation has
  // settled, and folds in a refresh generation that advances on every `appState.refresh()`. It
  // replaced a direct `status` dependency only so that a status array that is new by reference
  // but not the product of a refresh (a re-render, an optimistic update) doesn't refetch anything.
  //
  // `loadDiff` is pulled out of the effect (returning its own cleanup) so the failure banner's
  // Retry button can re-run the exact same fetch on demand, not just on a dependency change.
  const loadDiff = () => {
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
          setError(describeError(err));
        }
      });
    return () => {
      ignore = true;
    };
  };

  useEffect(() => {
    // Lazy: a collapsed section fetches nothing until it is expanded, and a section far from the
    // viewport fetches nothing until it is scrolled near (both PERF-001). `diffVersion` (not
    // `status`) is the staleness signal — see the comment on `loadDiff` above.
    if (mode !== "diff" || isConflicted || collapsed || !nearViewport) return;
    return loadDiff();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoPath, client, entry.path, entry.staged, isConflicted, mode, diffVersion, collapsed, nearViewport]);

  // `status` is a dependency for the same reason as the diff effect above: staging or committing
  // the file on screen while its blame view is open must not leave stale pre-commit attribution
  // on screen.
  const loadBlame = () => {
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
          setError({ message: "No blame available for this file at this revision." });
        }
      });
    return () => {
      ignore = true;
    };
  };

  useEffect(() => {
    if (mode !== "blame") return;
    return loadBlame();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoPath, client, entry.path, mode, status]);

  const Icon = STATUS_ICONS[entry.kind];

  return (
    <ListRow
      selected={isCurrent}
      onClick={onSelect}
      className={isConflicted ? `${styles.fileSection} ${styles.conflicted}` : styles.fileSection}
    >
      <div
        className={styles.fileSectionHeader}
        ref={(el) => {
          sectionElRef.current = el;
        }}
      >
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
                <InlineError
                  message={error.message}
                  hint={error.hint}
                  onDismiss={() => setError(null)}
                  onRetry={loadBlame}
                />
              ) : (
                <BlameView lines={blameLines} onSelectRow={onSelectRow} />
              )}
              <button type="button" onClick={() => setMode("diff")}>
                Back to diff
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
            <InlineError
              message={error.message}
              hint={error.hint}
              onDismiss={() => setError(null)}
              onRetry={loadDiff}
            />
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
  commits,
  refreshGeneration = 0,
  initialCommitDraft,
  onCommitDraftChange,
}: {
  repoPath: string;
  client: RepoClient;
  selectedRow: SelectedRow;
  // `useAppState`'s `state.refreshGeneration`: advances on every `refresh()`, so any refresh
  // (not just a hunk action taken in this pane) invalidates every open working-tree diff. Optional
  // only so tests that don't exercise refreshes needn't pass it.
  refreshGeneration?: number;
  // Passed straight through to `CommitBox` — see its `initialDraft`/`onDraftChange`.
  initialCommitDraft?: CommitDraft;
  onCommitDraftChange?: (draft: CommitDraft) => void;
  // Loaded history, used for the selected commit's header (author, date, parents).
  commits?: GraphCommit[];
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onStageAllFiles: (paths: string[]) => void;
  onUnstageAllFiles: (paths: string[]) => void;
  // May return the mutation's promise (as `useAppState`'s do): the pane waits for it to settle
  // before marking that path's diff stale, so the refetch can't race the mutation itself.
  onStageHunk: (path: string, oldStart: number, newStart: number) => void | Promise<void>;
  onUnstageHunk: (path: string, oldStart: number, newStart: number) => void | Promise<void>;
  onDiscardHunk: (path: string, oldStart: number, newStart: number) => void | Promise<void>;
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
        refreshGeneration={refreshGeneration}
        initialCommitDraft={initialCommitDraft}
        onCommitDraftChange={onCommitDraftChange}
      />
    );
  }
  return (
    <CommitDiffPane
      key={selectedRow.commitId}
      repoPath={repoPath}
      client={client}
      commitId={selectedRow.commitId}
      commits={commits}
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
  refreshGeneration,
  initialCommitDraft,
  onCommitDraftChange,
}: {
  repoPath: string;
  client: RepoClient;
  status: StatusEntry[];
  refreshGeneration: number;
  initialCommitDraft?: CommitDraft;
  onCommitDraftChange?: (draft: CommitDraft) => void;
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onStageAllFiles: (paths: string[]) => void;
  onUnstageAllFiles: (paths: string[]) => void;
  // May return the mutation's promise (as `useAppState`'s do): the pane waits for it to settle
  // before marking that path's diff stale, so the refetch can't race the mutation itself.
  onStageHunk: (path: string, oldStart: number, newStart: number) => void | Promise<void>;
  onUnstageHunk: (path: string, oldStart: number, newStart: number) => void | Promise<void>;
  onDiscardHunk: (path: string, oldStart: number, newStart: number) => void | Promise<void>;
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
  // Per-`entryKey` version counters: the per-path half of each section's staleness signal (the
  // other half is `refreshGeneration`, see `staleness` below). A hunk stage/unstage/discard bumps
  // *both* sides of the affected path — staging a hunk of a partially-staged file changes its
  // staged diff as much as its unstaged one, and either section may be open — and only once the
  // mutation's promise has settled, so the refetch can't observe the index before the backend
  // has applied the change.
  const [diffVersion, setDiffVersion] = useState<Record<string, number>>({});
  const bumpPathVersions = (path: string) => {
    const keys = [entryKey({ path, staged: false }), entryKey({ path, staged: true })];
    setDiffVersion((prev) => {
      const next = { ...prev };
      for (const key of keys) next[key] = (prev[key] ?? 0) + 1;
      return next;
    });
  };
  const wrapHunkAction =
    (action: (path: string, oldStart: number, newStart: number) => void | Promise<void>) =>
    (path: string, oldStart: number, newStart: number) => {
      // `useAppState`'s hunk actions never reject (`runMutation` records failures in
      // `state.error`), but `finally` keeps a rejecting caller from skipping the bump too — a
      // failed mutation may still have partially applied.
      void Promise.resolve(action(path, oldStart, newStart)).finally(() => bumpPathVersions(path));
    };
  // One number per section: the path's own version plus the pane-wide refresh generation. Both
  // only ever increase, so the sum changes whenever either does — and a hunk action, which bumps
  // both (its `runMutation` ends in a `refresh()`), still yields one refetch rather than two when
  // React batches the two updates into one render. Every open section refetches on any refresh:
  // that is deliberate, it's the only signal an external edit, stash apply or pull leaves behind
  // (there's no file watcher). PERF-001's savings come from lazy loading instead — collapsed and
  // off-screen sections never fetch at all.
  const staleness = (key: string) => (diffVersion[key] ?? 0) + refreshGeneration;

  useEffect(() => {
    if (current === null) return;
    sectionRefs.current[entryKey(current)]?.scrollIntoView?.({ block: "nearest" });
  }, [current]);

  const stagedEntries = status.filter((entry) => entry.staged);
  const unstagedEntries = status.filter((entry) => !entry.staged);
  const stagedCount = stagedEntries.length;
  const conflictCount = status.filter((entry) => entry.kind === "Conflicted").length;
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
        diffVersion={staleness(key)}
        onToggleCollapse={() => toggleCollapse(key)}
        onSelect={() => selectEntry(entry)}
        onStageFile={onStageFile}
        onUnstageFile={onUnstageFile}
        onStageHunk={wrapHunkAction(onStageHunk)}
        onUnstageHunk={wrapHunkAction(onUnstageHunk)}
        onDiscardHunk={wrapHunkAction(onDiscardHunk)}
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
            {conflictCount > 0 && (
              <span className={styles.conflictBadge}>
                {conflictCount} conflict{conflictCount === 1 ? "" : "s"}
              </span>
            )}
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
      <div className={styles.commitDock} role="group" aria-label="Commit actions">
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
            disabled={conflictCount > 0}
            disabledReason={conflictReason(conflictCount)}
            onContinue={onRebaseContinue}
            onAbort={onRebaseAbort}
          />
        ) : (
          <CommitBox
            onCommit={onCommit}
            disabled={stagedCount === 0 || conflictCount > 0}
            disabledReason={conflictCount > 0 ? conflictReason(conflictCount) : "Stage changes to commit"}
            onAbortMerge={onAbortMerge}
            initialMessage={mergeMessage ?? undefined}
            initialDraft={initialCommitDraft}
            onDraftChange={onCommitDraftChange}
          />
        )}
      </div>
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
  const [hunks, setHunks] = useState<DiffHunk[] | null>(null);
  const [blameLines, setBlameLines] = useState<BlameLine[]>([]);
  const [error, setError] = useState<DescribedError | null>(null);

  const loadDiff = () => {
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
          setError(describeError(err));
        }
      });
    return () => {
      ignore = true;
    };
  };

  useEffect(() => {
    if (mode !== "diff" || collapsed) return;
    return loadDiff();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoPath, client, commitId, path, mode, collapsed]);

  const loadBlame = () => {
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
          setError({ message: "No blame available for this file at this revision." });
        }
      });
    return () => {
      ignore = true;
    };
  };

  useEffect(() => {
    if (mode !== "blame") return;
    return loadBlame();
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
                <InlineError
                  message={error.message}
                  hint={error.hint}
                  onDismiss={() => setError(null)}
                  onRetry={loadBlame}
                />
              ) : (
                <BlameView lines={blameLines} onSelectRow={onSelectRow} />
              )}
              <button onClick={() => setMode("diff")}>Back to diff</button>
            </>
          ) : error !== null ? (
            <InlineError
              message={error.message}
              hint={error.hint}
              onDismiss={() => setError(null)}
              onRetry={loadDiff}
            />
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
  commits,
  onSelectRow,
}: {
  repoPath: string;
  client: RepoClient;
  commitId: string;
  commits?: GraphCommit[];
  onSelectRow: (row: SelectedRow) => void;
}) {
  const knownCommitIds = useMemo(() => new Set((commits ?? []).map((c) => c.id)), [commits]);
  const [files, setFiles] = useState<string[]>([]);
  const [error, setError] = useState<DescribedError | null>(null);
  const [collapsedPaths, setCollapsedPaths] = useState<Set<string>>(new Set());

  const loadFiles = () => {
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
          setError(describeError(err));
        }
      });
    return () => {
      ignore = true;
    };
  };

  useEffect(() => {
    return loadFiles();
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    return (
      <InlineError
        message={error.message}
        hint={error.hint}
        onDismiss={() => setError(null)}
        onRetry={loadFiles}
      />
    );
  }

  return (
    <div>
      <CommitHeader
        repoPath={repoPath}
        client={client}
        commitId={commitId}
        commit={commits?.find((c) => c.id === commitId)}
        knownCommitIds={knownCommitIds}
        onSelectRow={onSelectRow}
      />
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
