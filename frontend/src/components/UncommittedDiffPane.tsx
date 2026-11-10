import { CommitDock } from "./CommitDock";
import { UncommittedFileSection } from "./UncommittedFileSection";
import { NO_HEAD_INFO, withBlockedReason, type DiscardRequest, type SelectionActionKind } from "./diffPaneShared";
import { stashBlockReason } from "../lib/operationStatus";
import { useEffect, useId, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import type { GraphCommit, HeadInfo, HunkOptions, RepoClient, StatusEntry } from "../ipc/RepoClient";
import { useWorkspaceActions } from "../state/useWorkspaceActions";
import type { CommitDraft } from "./CommitBox";
import { CollapseAllToggle } from "./CollapseToggle";
import styles from "./DiffPane.module.css";
import {
  discardAllConfirmMessage,
  discardConfirmMessage,
  isUntracked,
  selectionMenuItems,
  type SelectionSummary,
} from "./FileActionsMenu";
import {
  EMPTY_FILE_SELECTION,
  extendFileSelection,
  selectAllFiles,
  selectOnlyFile,
  selectedPaths,
  toggleFile,
  type FileSelection,
} from "../lib/fileSelection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { isMenuKey } from "../lib/menuKeys";

// The change lists' keys (#449): arrows/j/k move, Shift extends, Ctrl/Cmd+A selects the group,
// Escape keeps only the current file, `s` stages or unstages, the Menu key opens the menu.
const FILE_LIST_SHORTCUTS =
  "ArrowUp ArrowDown Shift+ArrowUp Shift+ArrowDown Control+A Meta+A Escape s ContextMenu Shift+F10";

function entryKey(entry: { path: string; staged: boolean }): string {
  return `${entry.staged}:${entry.path}`;
}

export function UncommittedDiffPane({
  repoPath,
  client,
  commits,
  branchCommits,
  status,
  mergeMessage,
  amendMode = false,
  headInfo = NO_HEAD_INFO,
  focusCommitMessage = false,
  clearCommitMessage = null,
  rebaseProgress,
  sequencePending = false,
  refreshGeneration,
  discardRequest,
  onDiscardRequestChange: setDiscardRequest,
  initialCommitDraft,
  selectionBusyReason,
  discardBlockedReason,
}: {
  selectionBusyReason: string | null;
  discardBlockedReason: string | null;
  repoPath: string;
  client: RepoClient;
  status: StatusEntry[];
  refreshGeneration: number;
  // The open discard confirmation, owned by `RepoWorkspace` so the palette opens the same ones
  // (#493).
  discardRequest: DiscardRequest | null;
  onDiscardRequestChange: (request: DiscardRequest | null) => void;
  initialCommitDraft?: CommitDraft;
  /** The loaded history (may be a file history or search results); stands in for
   * `branchCommits` when that is absent. */
  commits?: GraphCommit[];
  /** The graph's commits (not a file history or search), for the branch's recent messages and
   * HEAD's author in the amend Author field. */
  branchCommits?: GraphCommit[];
  mergeMessage: string | null;
  amendMode?: boolean;
  headInfo?: HeadInfo;
  focusCommitMessage?: boolean;
  // One-shot CommitBox clear request (see its `clearMessage`).
  clearCommitMessage?: string | null;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  // An action is running: the sequence's Continue/Abort are disabled (no double submit).
  sequencePending?: boolean;
}) {
  const actions = useWorkspaceActions();
  const {
    onStageFile,
    onUnstageFile,
    onStageAllFiles,
    onUnstageAllFiles,
    onStageHunk,
    onUnstageHunk,
    onDiscardHunk,
    onStageLines,
    onUnstageLines,
    onDiscardLines,
    onDiscardFile,
    onDiscardAllUnstaged,
    onOpenStashDialog,
    onCurrentFileChange,
    onSelectionChange,
    patchActions,
  } = actions;
  const onDiscardFiles = actions.onDiscardFiles ?? onDiscardAllUnstaged;
  const onIgnorePaths = actions.onIgnorePaths ?? ((patterns: string[]) => patterns.forEach(actions.onIgnorePath));
  const [current, setCurrent] = useState<{ path: string; staged: boolean } | null>(null);
  // The files selected for a multi-file action (#449), within one group. The current file is
  // always part of it; a plain click or arrow key makes it the only one.
  const [selection, setSelection] = useState<FileSelection>(EMPTY_FILE_SELECTION);
  const rowIdPrefix = useId();
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
    (action: (path: string, oldStart: number, newStart: number, hunk: HunkOptions) => void | Promise<void>) =>
    (path: string, oldStart: number, newStart: number, hunk: HunkOptions) => {
      // `useAppState`'s hunk actions never reject (`runMutation` records failures in
      // `state.error`), but `finally` keeps a rejecting caller from skipping the bump too — a
      // failed mutation may still have partially applied.
      void Promise.resolve(action(path, oldStart, newStart, hunk)).finally(() => bumpPathVersions(path));
    };
  const wrapLineAction =
    (action: (path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]) => void | Promise<void>) =>
    (path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]) => {
      // Same settle-then-bump contract as `wrapHunkAction`.
      void Promise.resolve(action(path, oldStart, newStart, hunk, lineIndices, expectedLines)).finally(() =>
        bumpPathVersions(path),
      );
    };
  const wrapFileAction = (action: (path: string) => void | Promise<void>) => (path: string) => {
    // Same settle-then-bump contract as `wrapHunkAction`.
    void Promise.resolve(action(path)).finally(() => bumpPathVersions(path));
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

  const isEntryCurrent = (entry: StatusEntry) =>
    current !== null && current.path === entry.path && current.staged === entry.staged;
  const isEntrySelected = (entry: StatusEntry) =>
    selection.staged === entry.staged && selection.paths.has(entry.path);

  const groupEntries = (staged: boolean) => (staged ? stagedEntries : unstagedEntries);
  const groupPaths = (staged: boolean) => groupEntries(staged).map((entry) => entry.path);

  /** Makes `entry` the current file with `next` as the selection. */
  const focusEntry = (entry: StatusEntry, next: FileSelection) => {
    setCurrent({ path: entry.path, staged: entry.staged });
    setSelection(next);
  };
  const selectEntry = (entry: StatusEntry) => focusEntry(entry, selectOnlyFile(entry.staged, entry.path));

  // Ctrl/Cmd+click toggles a file, Shift+click selects the range from the anchor. Clicks inside a
  // file's diff keep their own meaning (Shift/Ctrl+click select diff lines there): they only make
  // the file current, keeping the selection when the file is already part of it.
  const handleRowClick = (entry: StatusEntry) => (event?: MouseEvent) => {
    const inBody = event?.target instanceof Element && event.target.closest("[data-file-body]") !== null;
    if (inBody) {
      if (isEntrySelected(entry)) setCurrent({ path: entry.path, staged: entry.staged });
      else selectEntry(entry);
      return;
    }
    if (event?.shiftKey) {
      focusEntry(entry, extendFileSelection(selection, entry.staged, groupPaths(entry.staged), entry.path));
    } else if (event?.ctrlKey || event?.metaKey) {
      const next = toggleFile(selection, entry.staged, entry.path);
      if (next.paths.has(entry.path)) {
        focusEntry(entry, next);
      } else {
        // Toggled off: the current file moves to the anchor (or none is left).
        setSelection(next);
        const anchor = groupEntries(entry.staged).find((candidate) => candidate.path === next.anchor);
        setCurrent(anchor === undefined ? null : { path: anchor.path, staged: anchor.staged });
      }
    } else {
      selectEntry(entry);
    }
  };

  const selectedEntries = (staged: boolean) =>
    selectedPaths(selection, staged, groupPaths(staged)).map(
      (path) => groupEntries(staged).find((entry) => entry.path === path) as StatusEntry,
    );
  // The selection's actions skip conflicted files, as "Stage all" and "Discard all…" do: staging a
  // conflicted file marks it resolved with whatever is in the working tree.
  const summaryOf = (staged: boolean): SelectionSummary | null => {
    const entries = selectedEntries(staged);
    if (entries.length < 2) return null;
    return {
      staged,
      actionable: entries.filter((entry) => entry.kind !== "Conflicted").length,
      untracked: entries.filter(isUntracked).length,
    };
  };
  const unstagedSummary = summaryOf(false);
  const stagedSummary = summaryOf(true);

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

  // Shift+arrow (`extend`) moves the current file and selects the range from the anchor to it.
  const navigateGroup = (entries: StatusEntry[], direction: 1 | -1, extend = false) => {
    if (entries.length === 0) return;
    const currentIndex = entries.findIndex(
      (entry) => current !== null && entry.path === current.path && entry.staged === current.staged,
    );
    const nextIndex =
      currentIndex === -1 ? 0 : Math.min(Math.max(currentIndex + direction, 0), entries.length - 1);
    const next = entries[nextIndex];
    if (!extend) {
      selectEntry(next);
      return;
    }
    // The range starts at the current file; without one in this group, at the file moved to.
    const anchorPath = entries[Math.max(currentIndex, 0)].path;
    const base =
      currentIndex !== -1 && selection.staged === next.staged && selection.paths.size > 0
        ? selection
        : selectOnlyFile(next.staged, anchorPath);
    focusEntry(next, extendFileSelection(base, next.staged, entries.map((entry) => entry.path), next.path));
  };

  const runSelectionAction = (staged: boolean, kind: SelectionActionKind) => {
    const entries = selectedEntries(staged);
    const paths = entries.filter((entry) => entry.kind !== "Conflicted").map((entry) => entry.path);
    if (kind === "stashSelection") {
      if (stashReason !== null || selectionBusyReason !== null || paths.length === 0) return;
      onOpenStashDialog(paths);
      return;
    }
    if (selectionBusyReason !== null) return;
    switch (kind) {
      case "stageSelection":
        if (paths.length === 0) return;
        onStageAllFiles(paths);
        break;
      case "unstageSelection":
        if (paths.length === 0) return;
        onUnstageAllFiles(paths);
        break;
      case "discardSelection":
        // Opens the confirmation; nothing is discarded until it is confirmed.
        if (paths.length > 0) setDiscardRequest({ kind: "files", paths });
        return;
      case "ignoreSelection": {
        const patterns = entries.filter(isUntracked).map((entry) => `/${entry.path}`);
        if (patterns.length === 0) return;
        onIgnorePaths(patterns);
        break;
      }
    }
    // The files leave this group (or the list): don't keep a selection that would come back if
    // they reappeared.
    setSelection(EMPTY_FILE_SELECTION);
  };

  // The per-row Stage/Unstage controls are `<button>`s nested inside a `role="option"` row. That
  // keeps them reachable by mouse and by plain Tab, but ARIA's listbox/option pattern treats an
  // option's children as content contributing to the option's name, not as independent widgets —
  // so assistive tech arrowing through this listbox does not reliably surface them. `s` is the
  // keyboard equivalent, on the container that already owns navigation (`j`/`k`/arrows), so the
  // whole stage/unstage flow is doable without ever reaching those buttons. See
  // `primitives/ListRow.tsx`'s doc comment for why the buttons weren't moved out of the row.
  const toggleStageSelected = (entries: StatusEntry[]) => {
    const staged = entries[0]?.staged;
    if (staged !== undefined && summaryOf(staged) !== null) {
      runSelectionAction(staged, staged ? "unstageSelection" : "stageSelection");
      return;
    }
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
    // The file menu and discard dialog render inside their row (React events bubble through the
    // tree), so keys typed in them must not drive the list underneath.
    if ((event.target as HTMLElement).closest('[role="menu"], dialog') !== null) return;
    // Inside a file's diff (or a text field) Ctrl/Cmd+A and Shift+arrows keep their own meaning.
    const inBody = (event.target as HTMLElement).closest("[data-file-body], input, textarea") !== null;
    const staged = entries[0]?.staged ?? false;
    if (event.key === "ArrowDown" || event.key === "j") {
      if (event.shiftKey && inBody) return;
      event.preventDefault();
      navigateGroup(entries, 1, event.shiftKey);
    } else if (event.key === "ArrowUp" || event.key === "k") {
      if (event.shiftKey && inBody) return;
      event.preventDefault();
      navigateGroup(entries, -1, event.shiftKey);
    } else if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "a") {
      if (inBody || entries.length === 0) return;
      event.preventDefault();
      const next = selectAllFiles(selection, staged, entries.map((entry) => entry.path));
      const keepCurrent = current !== null && current.staged === staged && next.paths.has(current.path);
      if (keepCurrent) setSelection(next);
      else focusEntry(entries[0], next);
    } else if (event.key === "Escape" && summaryOf(staged) !== null && current !== null && current.staged === staged) {
      // Back to just the current file.
      event.preventDefault();
      setSelection(selectOnlyFile(staged, current.path));
    } else if (event.key === "s") {
      event.preventDefault();
      toggleStageSelected(entries);
    } else if (isMenuKey(event)) {
      // Opens a row's actions menu by activating its "⋯" button: the row holding focus when focus
      // is on one of its controls (Tab can put it on a row that isn't selected), else the
      // selected row. A row without a "⋯" (no actions for it) makes this a no-op.
      const row =
        (event.target as HTMLElement).closest('[role="option"]') ??
        event.currentTarget.querySelector(`.${styles.current}`);
      const trigger = row?.querySelector<HTMLButtonElement>('button[aria-haspopup="menu"]') ?? null;
      if (trigger !== null) {
        event.preventDefault();
        trigger.click();
      }
    }
  };

  const discardableEntries = unstagedEntries.filter((entry) => entry.kind !== "Conflicted");
  // The selection's discard also follows the selection's own busy rule.
  const discardFilesBlockedReason = discardBlockedReason ?? selectionBusyReason;

  const discardFileEntry =
    discardRequest?.kind === "file"
      ? discardableEntries.find((entry) => entry.path === discardRequest.path)
      : undefined;
  // A selection's discard (#449): the requested files that can still be discarded.
  const discardFilesEntries =
    discardRequest?.kind === "files"
      ? discardableEntries.filter((entry) => discardRequest.paths.includes(entry.path))
      : [];

  // A request whose target is gone (refresh, staging, unmount) must not linger: otherwise the
  // destructive dialog would pop up unprompted when the path later reappears.
  const requestTargetGone =
    discardRequest !== null &&
    (discardRequest.kind === "all"
      ? discardableEntries.length === 0
      : discardRequest.kind === "files"
        ? discardFilesEntries.length === 0
        : discardFileEntry === undefined);
  useEffect(() => {
    if (requestTargetGone) setDiscardRequest(null);
  }, [requestTargetGone, setDiscardRequest]);
  const setDiscardRequestRef = useRef(setDiscardRequest);
  useEffect(() => {
    setDiscardRequestRef.current = setDiscardRequest;
  });
  useEffect(() => () => setDiscardRequestRef.current(null), []);

  // Report the current file to the palette; only unstaged, non-conflicted entries can be discarded.
  const currentDiscardablePath =
    current !== null && !current.staged
      ? (discardableEntries.find((entry) => entry.path === current.path)?.path ?? null)
      : null;
  useEffect(() => {
    onCurrentFileChange?.(currentDiscardablePath);
  }, [currentDiscardablePath, onCurrentFileChange]);
  useEffect(() => () => onCurrentFileChange?.(null), [onCurrentFileChange]);

  const handleStageAll = () => {
    onStageAllFiles(stageAllPaths);
  };

  const handleUnstageAll = () => {
    onUnstageAllFiles(stagedEntries.map((entry) => entry.path));
  };

  const stashReason = stashBlockReason({ status, rebaseProgress, headInfo });

  // Reports a selection of two or more files to the command palette; keyed on its content so an
  // unchanged selection reports nothing new.
  const reportedSelection = unstagedSummary !== null ? false : stagedSummary !== null ? true : null;
  // Conflicted files are left out, as the selection's actions leave them out.
  const reportedPaths =
    reportedSelection === null
      ? []
      : selectedEntries(reportedSelection)
          .filter((entry) => entry.kind !== "Conflicted")
          .map((entry) => entry.path);
  const reportedKey =
    reportedSelection === null || reportedPaths.length === 0 ? "" : `${reportedSelection}\0${reportedPaths.join("\0")}`;
  useEffect(() => {
    if (reportedKey === "") {
      onSelectionChange?.(null);
      return;
    }
    const [staged, ...paths] = reportedKey.split("\0");
    onSelectionChange?.({ staged: staged === "true", paths });
  }, [reportedKey, onSelectionChange]);
  useEffect(() => () => onSelectionChange?.(null), [onSelectionChange]);

  // What the live region announces: the size of a multi-file selection, or the one current file.
  const selectionAnnouncement = (() => {
    for (const staged of [false, true]) {
      const paths = selectedPaths(selection, staged, groupPaths(staged));
      if (paths.length >= 2) return `${paths.length} files selected in ${staged ? "Staged" : "Changes"}`;
      if (paths.length === 1) return `${paths[0]} selected`;
    }
    return "";
  })();

  const renderSelectionBar = (summary: SelectionSummary) => (
    <div
      className={styles.selectionBar}
      role="group"
      aria-label={summary.staged ? "Selected staged files" : "Selected unstaged files"}
    >
      <span>{selectedPaths(selection, summary.staged, groupPaths(summary.staged)).length} selected</span>
      {selectionMenuItems(
        summary,
        (action) => runSelectionAction(summary.staged, action.kind as SelectionActionKind),
        selectionBusyReason,
        stashReason,
      ).map((item) => (
        <button key={item.label} type="button" onClick={item.onSelect} disabled={item.disabled} title={item.title}>
          {item.label}
        </button>
      ))}
      <button
        type="button"
        onClick={() =>
          current !== null && current.staged === summary.staged
            ? setSelection(selectOnlyFile(summary.staged, current.path))
            : setSelection(EMPTY_FILE_SELECTION)
        }
      >
        Clear selection
      </button>
    </div>
  );

  const renderSection = (entry: StatusEntry, index: number) => {
    const key = entryKey(entry);
    const summary = isEntrySelected(entry) ? (entry.staged ? stagedSummary : unstagedSummary) : null;
    return (
      <UncommittedFileSection
        key={key}
        repoPath={repoPath}
        client={client}
        entry={entry}
        status={status}
        isCurrent={isEntryCurrent(entry)}
        selected={isEntrySelected(entry)}
        rowId={`${rowIdPrefix}-${entry.staged ? "staged" : "unstaged"}-${index}`}
        collapsed={collapsedKeys.has(key)}
        diffVersion={staleness(key)}
        onToggleCollapse={() => toggleCollapse(key)}
        onSelect={handleRowClick(entry)}
        selectionSummary={summary}
        onSelectionAction={(kind) => runSelectionAction(entry.staged, kind)}
        selectionBusyReason={selectionBusyReason}
        onStageHunk={wrapHunkAction(onStageHunk)}
        onUnstageHunk={wrapHunkAction(onUnstageHunk)}
        onDiscardHunk={wrapHunkAction(onDiscardHunk)}
        onStageLines={wrapLineAction(onStageLines)}
        onUnstageLines={wrapLineAction(onUnstageLines)}
        onDiscardLines={wrapLineAction(onDiscardLines)}
        onRequestDiscardFile={(path) => setDiscardRequest({ kind: "file", path })}
        onStashFile={(path) => onOpenStashDialog([path])}
        stashDisabledReason={stashReason}
        sectionRef={(el) => {
          sectionRefs.current[key] = el;
        }}
      />
    );
  };

  return (
    <div>
      <span aria-live="polite" aria-atomic="true" className={styles.srOnly} data-testid="file-selection-announcement">
        {selectionAnnouncement}
      </span>
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
            <button
              type="button"
              onClick={() => setDiscardRequest({ kind: "all" })}
              disabled={discardableEntries.length === 0}
            >
              Discard all…
            </button>
          </div>
          {unstagedSummary !== null && renderSelectionBar(unstagedSummary)}
          <ul
            className={styles.fileList}
            role="listbox"
            aria-multiselectable="true"
            aria-label="Unstaged changes"
            aria-keyshortcuts={FILE_LIST_SHORTCUTS}
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
            {patchActions !== undefined && (
              <button type="button" onClick={() => patchActions.onSavePatch({ kind: "Staged" }, "staged.patch")}>
                Save as patch…
              </button>
            )}
          </div>
          {stagedSummary !== null && renderSelectionBar(stagedSummary)}
          <ul
            className={styles.fileList}
            role="listbox"
            aria-multiselectable="true"
            aria-label="Staged changes"
            aria-keyshortcuts={FILE_LIST_SHORTCUTS}
            tabIndex={0}
            onKeyDown={handleGroupKeyDown(stagedEntries)}
          >
            {stagedEntries.map(renderSection)}
          </ul>
        </div>
      )}
      {discardRequest?.kind === "all" && discardableEntries.length > 0 && (
        <ConfirmDialog
          ariaLabel="Discard all changes"
          message={withBlockedReason(discardAllConfirmMessage(discardableEntries), discardBlockedReason)}
          confirmLabel="Discard all"
          confirmDisabled={discardBlockedReason !== null}
          confirmTitle={discardBlockedReason ?? undefined}
          onConfirm={() => {
            if (discardBlockedReason !== null) return;
            setDiscardRequest(null);
            onDiscardAllUnstaged(discardableEntries.map((e) => e.path));
          }}
          onCancel={() => setDiscardRequest(null)}
        />
      )}
      {discardRequest?.kind === "files" && discardFilesEntries.length > 0 && (
        <ConfirmDialog
          ariaLabel="Discard selected changes"
          message={withBlockedReason(discardAllConfirmMessage(discardFilesEntries), discardFilesBlockedReason)}
          confirmLabel={`Discard ${discardFilesEntries.length} ${discardFilesEntries.length === 1 ? "file" : "files"}`}
          confirmDisabled={discardFilesBlockedReason !== null}
          confirmTitle={discardFilesBlockedReason ?? undefined}
          onConfirm={() => {
            if (discardFilesBlockedReason !== null) return;
            setDiscardRequest(null);
            setSelection(EMPTY_FILE_SELECTION);
            onDiscardFiles(discardFilesEntries.map((entry) => entry.path));
          }}
          onCancel={() => setDiscardRequest(null)}
        />
      )}
      {discardFileEntry !== undefined && (
        <ConfirmDialog
          ariaLabel="Discard changes"
          message={withBlockedReason(discardConfirmMessage(discardFileEntry), discardBlockedReason)}
          confirmLabel={isUntracked(discardFileEntry) ? "Delete file" : "Discard changes"}
          confirmDisabled={discardBlockedReason !== null}
          confirmTitle={discardBlockedReason ?? undefined}
          onConfirm={() => {
            if (discardBlockedReason !== null) return;
            setDiscardRequest(null);
            wrapFileAction(onDiscardFile)(discardFileEntry.path);
          }}
          onCancel={() => setDiscardRequest(null)}
        />
      )}
      <CommitDock
        repoPath={repoPath}
        client={client}
        commits={commits}
        branchCommits={branchCommits}
        mergeMessage={mergeMessage}
        amendMode={amendMode}
        headInfo={headInfo}
        rebaseProgress={rebaseProgress}
        sequencePending={sequencePending}
        stagedCount={stagedCount}
        conflictCount={conflictCount}
        stashReason={stashReason}
        initialCommitDraft={initialCommitDraft}
        focusCommitMessage={focusCommitMessage}
        clearCommitMessage={clearCommitMessage}
      />
    </div>
  );
}
