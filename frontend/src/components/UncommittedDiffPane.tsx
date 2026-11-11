import { CommitDock } from "./CommitDock";
import { UncommittedFileRow } from "./UncommittedFileRow";
import { UncommittedFileViewer } from "./UncommittedFileViewer";
import { DiffOptionsToolbar } from "./DiffOptionsToolbar";
import { NO_HEAD_INFO, type DiscardRequest, type SelectionActionKind } from "./diffPaneShared";
import { stashBlockReason } from "../lib/operationStatus";
import { useEffect, useId, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import type { GraphCommit, HeadInfo, HunkOptions, RepoClient, StatusEntry } from "../ipc/RepoClient";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { useWorkspaceActions } from "../state/useWorkspaceActions";
import type { CommitDraft } from "./CommitBox";
import styles from "./DiffPane.module.css";
import {
  DELETE_UNTRACKED_PREVIEW,
  deleteUntrackedConfirmMessage,
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
import { stageAllPaths, stageModifiedPaths, untrackedPaths } from "../lib/stagePaths";

// The change lists' keys (#449): arrows/j/k move (on to the other group past either end, #639),
// Shift extends, Ctrl/Cmd+A selects the group, Escape keeps only the current file, `s` stages or
// unstages, Enter moves into the diff, the Menu key opens the menu.
const FILE_LIST_SHORTCUTS =
  "ArrowUp ArrowDown Shift+ArrowUp Shift+ArrowDown Control+A Meta+A Escape s Enter ContextMenu Shift+F10";

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
  conflictRequest = false,
  onConflictRequestHandled,
  statusLoaded = true,
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
  /**
   * "Go to conflict" from the operation strip (#665): the first conflicted file becomes the
   * current one, its resolution focused; then `onConflictRequestHandled`.
   */
  conflictRequest?: boolean;
  onConflictRequestHandled?: () => void;
  /** Until the first status read is in, an empty list says nothing (no "clean" claim). */
  statusLoaded?: boolean;
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
  const onDeleteUntracked = actions.onDeleteUntracked ?? onDiscardFiles;
  const onIgnorePaths = actions.onIgnorePaths ?? ((patterns: string[]) => patterns.forEach(actions.onIgnorePath));
  const [current, setCurrent] = useState<{ path: string; staged: boolean } | null>(null);
  // The files selected for a multi-file action (#449), within one group. The current file is
  // always part of it; a plain click or arrow key makes it the only one.
  const [selection, setSelection] = useState<FileSelection>(EMPTY_FILE_SELECTION);
  const rowIdPrefix = useId();
  // The untracked files whose contents the user asked for while "Expand untracked files" is off
  // (#623), by `entryKey`; kept across refreshes while the pane is open.
  const [revealedUntracked, setRevealedUntracked] = useState<ReadonlySet<string>>(new Set());
  const { expandUntracked } = useDiffSettingsContext().settings;
  const contentsHidden = (entry: StatusEntry) =>
    !expandUntracked && isUntracked(entry) && !revealedUntracked.has(entryKey(entry));
  const rowRefs = useRef<Record<string, HTMLElement | null>>({});
  // Asks the viewer to take focus once it shows `key` (Enter in a list, j/k in the viewer). Any
  // other way of changing the current file drops it, so a later visit doesn't steal focus.
  const [focusRequest, setFocusRequest] = useState<{ key: string; n: number } | null>(null);
  // The first changed line of the current file's loaded diff, for its row's "Open in editor".
  const [firstChangedLine, setFirstChangedLine] = useState<{ key: string; line: number | null } | null>(null);
  // Per-`entryKey` version counters: the per-path half of the viewer's staleness signal (the
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
  // One number per file: the path's own version plus the pane-wide refresh generation. Both only
  // ever increase, so the sum changes whenever either does — and a hunk action, which bumps both
  // (its `runMutation` ends in a `refresh()`), still yields one refetch rather than two when React
  // batches the two updates into one render. The viewer refetches on any refresh: that is
  // deliberate, it's the only signal an external edit, stash apply or pull leaves behind (changes
  // made outside the app reach it as a refresh too, `useExternalChanges`, #650). Only the
  // current file's diff is ever fetched (#639).
  const staleness = (key: string) => (diffVersion[key] ?? 0) + refreshGeneration;

  const stagedEntries = status.filter((entry) => entry.staged);
  // Conflicts head Changes, in a group of their own (#665).
  const conflictEntries = status.filter((entry) => !entry.staged && entry.kind === "Conflicted");
  const unstagedEntries = [...conflictEntries, ...status.filter((entry) => !entry.staged && entry.kind !== "Conflicted")];
  const stagedCount = stagedEntries.length;
  const conflictCount = status.filter((entry) => entry.kind === "Conflicted").length;
  // Both bulk actions skip conflicts (see `lib/stagePaths`). "Stage modified" (#621) also skips
  // untracked files; it is shown only when the group holds both kinds, as otherwise it would do
  // what "Stage all" does.
  const allStagePaths = stageAllPaths(status);
  const modifiedStagePaths = stageModifiedPaths(status);
  // "Delete untracked…" (#622) deletes exactly these: tracked changes and conflicts stay.
  const deletableUntrackedPaths = untrackedPaths(status);
  const untrackedCount = deletableUntrackedPaths.length;
  const showStageModified = untrackedCount > 0 && unstagedEntries.some((entry) => entry.kind !== "New");

  // List order: Changes, then Staged.
  const allEntries = [...unstagedEntries, ...stagedEntries];

  // The file the viewer shows. When the chosen one is gone (staged, unstaged, discarded,
  // committed), the file now at its place in the same group takes over, else the other group's
  // first; with nothing chosen yet, the first file.
  // Where the current file was in its group at the last render, for the fallback.
  const [lastPlace, setLastPlace] = useState<{ staged: boolean; index: number } | null>(null);
  const findEntry = (target: { path: string; staged: boolean } | null) =>
    target === null ? undefined : allEntries.find((entry) => entry.path === target.path && entry.staged === target.staged);
  const fallbackEntry = (): StatusEntry | undefined => {
    const place = lastPlace;
    if (place === null) return unstagedEntries[0] ?? stagedEntries[0];
    const same = place.staged ? stagedEntries : unstagedEntries;
    const other = place.staged ? unstagedEntries : stagedEntries;
    return same[Math.min(place.index, same.length - 1)] ?? other[0];
  };
  const chosenEntry = findEntry(current);
  const currentEntry = chosenEntry ?? fallbackEntry();
  // Derived state, updated during render (as `DiffView` resets its selection) rather than in an
  // effect: the fallback becomes the current file and its selection at once, so keys and the
  // palette act on what the viewer shows.
  if (currentEntry !== undefined) {
    const index = (currentEntry.staged ? stagedEntries : unstagedEntries).indexOf(currentEntry);
    if (lastPlace?.staged !== currentEntry.staged || lastPlace.index !== index) {
      setLastPlace({ staged: currentEntry.staged, index });
    }
    if (chosenEntry === undefined) {
      setCurrent({ path: currentEntry.path, staged: currentEntry.staged });
      if (!(selection.staged === currentEntry.staged && selection.paths.has(currentEntry.path))) {
        setSelection(selectOnlyFile(currentEntry.staged, currentEntry.path));
      }
    }
  }
  const currentKey = currentEntry === undefined ? null : entryKey(currentEntry);

  useEffect(() => {
    if (currentKey === null) return;
    rowRefs.current[currentKey]?.scrollIntoView?.({ block: "nearest" });
  }, [currentKey]);

  const isEntryCurrent = (entry: StatusEntry) => currentEntry === entry;
  const isEntrySelected = (entry: StatusEntry) =>
    selection.staged === entry.staged && selection.paths.has(entry.path);

  const groupEntries = (staged: boolean) => (staged ? stagedEntries : unstagedEntries);
  const groupPaths = (staged: boolean) => groupEntries(staged).map((entry) => entry.path);

  /** Makes `entry` the current file with `next` as the selection. */
  const focusEntry = (entry: StatusEntry, next: FileSelection) => {
    setCurrent({ path: entry.path, staged: entry.staged });
    setSelection(next);
    setFocusRequest(null);
  };
  const selectEntry = (entry: StatusEntry) => focusEntry(entry, selectOnlyFile(entry.staged, entry.path));

  // "Go to conflict" (#665): once per request, the first conflict becomes the current file with
  // the focus in its resolution; the request is then handed back.
  const [conflictRequestSeen, setConflictRequestSeen] = useState(false);
  if (conflictRequest !== conflictRequestSeen) {
    setConflictRequestSeen(conflictRequest);
    const first = conflictEntries[0];
    if (conflictRequest && first !== undefined) {
      selectEntry(first);
      setFocusRequest((prev) => ({ key: entryKey(first), n: (prev?.n ?? 0) + 1 }));
    }
  }
  useEffect(() => {
    if (conflictRequest && conflictRequestSeen) onConflictRequestHandled?.();
  }, [conflictRequest, conflictRequestSeen, onConflictRequestHandled]);

  // Ctrl/Cmd+click toggles a file, Shift+click selects the range from the anchor.
  const handleRowClick = (entry: StatusEntry) => (event?: MouseEvent) => {
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

  const listRefs = useRef<{ staged: HTMLUListElement | null; unstaged: HTMLUListElement | null }>({
    staged: null,
    unstaged: null,
  });

  // Shift+arrow (`extend`) moves the current file and selects the range from the anchor to it.
  // A plain move past either end of a group goes on to the other group (#639), and focus with it.
  const navigateGroup = (entries: StatusEntry[], direction: 1 | -1, extend = false) => {
    if (entries.length === 0) return;
    const currentIndex = entries.findIndex((entry) => entry === currentEntry);
    const staged = entries[0].staged;
    const other = staged ? unstagedEntries : stagedEntries;
    const pastEnd = currentIndex !== -1 && currentIndex + direction >= entries.length;
    const pastStart = currentIndex !== -1 && currentIndex + direction < 0;
    if (!extend && (pastEnd || pastStart) && other.length > 0) {
      selectEntry(pastEnd ? other[0] : other[other.length - 1]);
      listRefs.current[staged ? "unstaged" : "staged"]?.focus();
      return;
    }
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
    const entry = entries.find((candidate) => candidate === currentEntry);
    if (entry === undefined) return;
    if (entry.staged) {
      onUnstageFile(entry.path);
    } else {
      onStageFile(entry.path);
    }
  };

  const handleGroupKeyDown = (entries: StatusEntry[]) => (event: KeyboardEvent<HTMLUListElement>) => {
    // The file menu and dialogs render inside their row (React events bubble through the tree),
    // so keys typed in them must not drive the list underneath.
    if ((event.target as HTMLElement).closest('[role="menu"], dialog') !== null) return;
    const staged = entries[0]?.staged ?? false;
    if (event.key === "ArrowDown" || event.key === "j") {
      event.preventDefault();
      navigateGroup(entries, 1, event.shiftKey);
    } else if (event.key === "ArrowUp" || event.key === "k") {
      event.preventDefault();
      navigateGroup(entries, -1, event.shiftKey);
    } else if (event.key === "Enter" && event.target === event.currentTarget && currentKey !== null) {
      event.preventDefault();
      setFocusRequest((prev) => ({ key: currentKey, n: (prev?.n ?? 0) + 1 }));
    } else if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "a") {
      if (entries.length === 0) return;
      event.preventDefault();
      const next = selectAllFiles(selection, staged, entries.map((entry) => entry.path));
      const keepCurrent = currentEntry !== undefined && currentEntry.staged === staged && next.paths.has(currentEntry.path);
      if (keepCurrent) setSelection(next);
      else focusEntry(entries[0], next);
    } else if (event.key === "Escape" && summaryOf(staged) !== null && currentEntry?.staged === staged) {
      // Back to just the current file.
      event.preventDefault();
      setSelection(selectOnlyFile(staged, currentEntry.path));
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
      : discardRequest.kind === "untracked"
        ? deletableUntrackedPaths.length === 0
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
    currentEntry !== undefined && !currentEntry.staged && currentEntry.kind !== "Conflicted" ? currentEntry.path : null;
  useEffect(() => {
    onCurrentFileChange?.(currentDiscardablePath);
  }, [currentDiscardablePath, onCurrentFileChange]);
  useEffect(() => () => onCurrentFileChange?.(null), [onCurrentFileChange]);

  const handleStageAll = () => {
    onStageAllFiles(allStagePaths);
  };

  const handleStageModified = () => {
    onStageAllFiles(modifiedStagePaths);
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
          currentEntry !== undefined && currentEntry.staged === summary.staged
            ? setSelection(selectOnlyFile(summary.staged, currentEntry.path))
            : setSelection(EMPTY_FILE_SELECTION)
        }
      >
        Clear selection
      </button>
    </div>
  );

  const renderRow = (entry: StatusEntry, index: number) => {
    const key = entryKey(entry);
    const summary = isEntrySelected(entry) ? (entry.staged ? stagedSummary : unstagedSummary) : null;
    return (
      <UncommittedFileRow
        key={key}
        entry={entry}
        isCurrent={isEntryCurrent(entry)}
        selected={isEntrySelected(entry)}
        rowId={`${rowIdPrefix}-${entry.staged ? "staged" : "unstaged"}-${index}`}
        onSelect={handleRowClick(entry)}
        selectionSummary={summary}
        onSelectionAction={(kind) => runSelectionAction(entry.staged, kind)}
        selectionBusyReason={selectionBusyReason}
        onRequestDiscardFile={(path) => setDiscardRequest({ kind: "file", path })}
        onStashFile={(path) => onOpenStashDialog([path])}
        stashDisabledReason={stashReason}
        editorLine={() => (firstChangedLine?.key === key ? firstChangedLine.line : null)}
        rowRef={(el) => {
          rowRefs.current[key] = el;
        }}
      />
    );
  };

  // j/k in the viewer: the next or previous file in list order, focus staying in the diff.
  const navigateFromViewer = (direction: 1 | -1) => {
    if (currentEntry === undefined) return;
    const next = allEntries[allEntries.indexOf(currentEntry) + direction];
    if (next === undefined) return;
    selectEntry(next);
    setFocusRequest((prev) => ({ key: entryKey(next), n: (prev?.n ?? 0) + 1 }));
  };
  const reportFirstLine = (line: number | null) => {
    if (currentKey !== null) setFirstChangedLine({ key: currentKey, line });
  };

  const renderGroup = (staged: boolean) => {
    const entries = groupEntries(staged);
    const summary = staged ? stagedSummary : unstagedSummary;
    return (
      <div className={styles.group}>
        {summary !== null && renderSelectionBar(summary)}
        {entries.length === 0 ? (
          <p className={styles.emptyGroup}>{staged ? "Nothing staged yet." : "No unstaged changes."}</p>
        ) : (
          <ul
            ref={(el) => {
              listRefs.current[staged ? "staged" : "unstaged"] = el;
            }}
            className={`${styles.fileList} ${styles.groupList}`}
            role="listbox"
            aria-multiselectable="true"
            aria-label={staged ? "Staged changes" : "Unstaged changes"}
            aria-keyshortcuts={FILE_LIST_SHORTCUTS}
            tabIndex={0}
            onKeyDown={handleGroupKeyDown(entries)}
          >
            {!staged && conflictEntries.length > 0 ? (
              // Conflicts first, in a group of their own (#665); the keys move through both.
              <>
                <li role="group" aria-labelledby={`${rowIdPrefix}-conflicts`} className={styles.subgroup}>
                  <span id={`${rowIdPrefix}-conflicts`} className={styles.subgroupHeading}>
                    Conflicts ({conflictEntries.length}) · resolve these first
                  </span>
                  <ul role="none" className={styles.fileList}>
                    {conflictEntries.map(renderRow)}
                  </ul>
                </li>
                {entries.length > conflictEntries.length && (
                  <li role="group" aria-labelledby={`${rowIdPrefix}-other-changes`} className={styles.subgroup}>
                    <span id={`${rowIdPrefix}-other-changes`} className={styles.subgroupHeading}>
                      Other changes
                    </span>
                    <ul role="none" className={styles.fileList}>
                      {entries.slice(conflictEntries.length).map((entry, index) => renderRow(entry, conflictEntries.length + index))}
                    </ul>
                  </li>
                )}
              </>
            ) : (
              entries.map(renderRow)
            )}
          </ul>
        )}
      </div>
    );
  };

  return (
    <div className={styles.workingTree}>
      <span aria-live="polite" aria-atomic="true" className={styles.srOnly} data-testid="file-selection-announcement">
        {selectionAnnouncement}
      </span>
      {allEntries.length === 0 && statusLoaded && (
        // Nothing to list (#665): say the tree is clean, or how a new repository gets its first
        // commit, instead of an empty pane.
        <div className={styles.emptyState} role="status">
          {headInfo.headId === null ? (
            <>
              <p className={styles.emptyStateTitle}>No commits yet</p>
              <p>Add files to the repository folder: they appear here. Stage them and commit to make the first commit.</p>
            </>
          ) : (
            <>
              <p className={styles.emptyStateTitle}>Working tree clean</p>
              <p>Nothing to commit. Changes to files in the repository folder appear here.</p>
            </>
          )}
        </div>
      )}
      {allEntries.length > 0 && (
        <div className={styles.fileGroups}>
          <div className={styles.groupHeading}>
            <span>Changes ({unstagedEntries.length})</span>
            {conflictCount > 0 && (
              <span className={styles.conflictBadge}>
                {conflictCount} conflict{conflictCount === 1 ? "" : "s"}
              </span>
            )}
            {/* Disabled when every unstaged entry is a conflict — the action would be a no-op
                that still costs a full refresh. */}
            <button
              type="button"
              onClick={handleStageAll}
              disabled={allStagePaths.length === 0}
              title={showStageModified ? "Stage every change, untracked files included (conflicts are left out)" : undefined}
            >
              Stage all
            </button>
            {showStageModified && (
              <button
                type="button"
                onClick={handleStageModified}
                disabled={modifiedStagePaths.length === 0}
                title={
                  modifiedStagePaths.length === 0
                    ? "No tracked file has changes to stage; conflicts are left out"
                    : `Stage the changes to tracked files only; "Stage all" also stages the ${untrackedCount} untracked file${untrackedCount === 1 ? "" : "s"}`
                }
              >
                Stage modified
              </button>
            )}
            <button
              type="button"
              onClick={() => setDiscardRequest({ kind: "all" })}
              disabled={discardableEntries.length === 0}
            >
              Discard all…
            </button>
            {untrackedCount > 0 && (
              <button
                type="button"
                onClick={() => setDiscardRequest({ kind: "untracked" })}
                title={`Delete the ${untrackedCount} untracked ${untrackedCount === 1 ? "file" : "files"} and keep the changes to tracked files`}
              >
                Delete untracked…
              </button>
            )}
          </div>
          {renderGroup(false)}
          <div className={styles.groupHeading}>
            <span>Staged ({stagedEntries.length})</span>
            <button type="button" onClick={handleUnstageAll} disabled={stagedEntries.length === 0}>
              Unstage all
            </button>
            {patchActions !== undefined && (
              <button
                type="button"
                disabled={stagedEntries.length === 0}
                onClick={() => patchActions.onSavePatch({ kind: "Staged" }, "staged.patch")}
              >
                Save as patch…
              </button>
            )}
          </div>
          {renderGroup(true)}
        </div>
      )}
      {currentEntry !== undefined && (
        <>
          <DiffOptionsToolbar sticky />
          <UncommittedFileViewer
            key={currentKey}
            repoPath={repoPath}
            client={client}
            entry={currentEntry}
            status={status}
            diffVersion={staleness(currentKey as string)}
            contentsHidden={contentsHidden(currentEntry)}
            onShowContents={() => setRevealedUntracked((prev) => new Set(prev).add(currentKey as string))}
            focusKey={focusRequest !== null && focusRequest.key === currentKey ? focusRequest.n : 0}
            onNavigate={navigateFromViewer}
            onFirstChangedLine={reportFirstLine}
            onStageHunk={wrapHunkAction(onStageHunk)}
            onUnstageHunk={wrapHunkAction(onUnstageHunk)}
            onDiscardHunk={wrapHunkAction(onDiscardHunk)}
            onStageLines={wrapLineAction(onStageLines)}
            onUnstageLines={wrapLineAction(onUnstageLines)}
            onDiscardLines={wrapLineAction(onDiscardLines)}
          />
        </>
      )}
      {discardRequest?.kind === "all" && discardableEntries.length > 0 && (
        <ConfirmDialog
          ariaLabel="Discard all changes"
          message={discardAllConfirmMessage(discardableEntries, true)}
          confirmLabel="Discard all"
          confirmReason={discardBlockedReason}
          onConfirm={() => {
            if (discardBlockedReason !== null) return;
            setDiscardRequest(null);
            onDiscardAllUnstaged(discardableEntries.map((e) => e.path));
          }}
          onCancel={() => setDiscardRequest(null)}
        />
      )}
      {discardRequest?.kind === "untracked" && untrackedCount > 0 && (
        <ConfirmDialog
          ariaLabel="Delete untracked files"
          message={
            <>
              <p>{deleteUntrackedConfirmMessage(deletableUntrackedPaths)}</p>
              <ul aria-label="Untracked files to delete">
                {deletableUntrackedPaths.slice(0, DELETE_UNTRACKED_PREVIEW).map((path) => (
                  <li key={path}>{path}</li>
                ))}
                {untrackedCount > DELETE_UNTRACKED_PREVIEW && (
                  <li>and {untrackedCount - DELETE_UNTRACKED_PREVIEW} more</li>
                )}
              </ul>
            </>
          }
          confirmLabel={`Delete ${untrackedCount} ${untrackedCount === 1 ? "file" : "files"}`}
          confirmReason={discardBlockedReason}
          onConfirm={() => {
            if (discardBlockedReason !== null) return;
            setDiscardRequest(null);
            onDeleteUntracked(deletableUntrackedPaths);
          }}
          onCancel={() => setDiscardRequest(null)}
        />
      )}
      {discardRequest?.kind === "files" && discardFilesEntries.length > 0 && (
        <ConfirmDialog
          ariaLabel="Discard selected changes"
          message={discardAllConfirmMessage(discardFilesEntries)}
          confirmLabel={`Discard ${discardFilesEntries.length} ${discardFilesEntries.length === 1 ? "file" : "files"}`}
          confirmReason={discardFilesBlockedReason}
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
          message={discardConfirmMessage(discardFileEntry)}
          confirmLabel={isUntracked(discardFileEntry) ? "Delete file" : "Discard changes"}
          confirmReason={discardBlockedReason}
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
        initialCommitDraft={initialCommitDraft}
        focusCommitMessage={focusCommitMessage}
        clearCommitMessage={clearCommitMessage}
      />
    </div>
  );
}
