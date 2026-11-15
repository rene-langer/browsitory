import type { CommitView, DiffPanePatchActions, FileHistoryFocus } from "./diffPaneShared";
import { describeError, type DescribedError } from "../lib/errorMessages";
import { useEffect, useId, useMemo, useState } from "react";
import type {
  ChangedFile,
  CommitHunk,
  CommitStats,
  DiffOptions,
  DiffSource,
  FileStats,
  ForgeRepository,
  GraphCommit,
  RepoClient,
} from "../ipc/RepoClient";
import { useWorkspaceActions } from "../state/useWorkspaceActions";
import { CollapseAllToggle } from "./CollapseToggle";
import { DiffOptionsToolbar } from "./DiffOptionsToolbar";
import { CommitHeader } from "./CommitHeader";
import styles from "./DiffPane.module.css";
import { InlineError } from "./primitives/InlineError";
import { ReadOnlyFileSection, type CommitFilePatchActions } from "./ReadOnlyFileSection";
import { CommitTreeBrowser } from "./CommitTreeBrowser";
import treeStyles from "./CommitTreeBrowser.module.css";

/** A commit's file's `CommitFilePatchActions`, from `DiffPane`'s. */
function commitFilePatch(
  actions: DiffPanePatchActions,
  commitId: string,
  path: string,
  oldPath: string | null,
): CommitFilePatchActions {
  const change = (reverse: boolean, hunk: CommitHunk | null, stage: boolean) =>
    actions.onApplyCommitChange({ commitId, path, oldPath, hunk, reverse, stage });
  const name = path.split("/").pop() ?? path;
  return {
    blockedReason: actions.blockedReason,
    onRevertFile: (stage) => change(true, null, stage),
    onApplyFile: (stage) => change(false, null, stage),
    onSavePatch: () =>
      actions.onSavePatch({ kind: "CommitFile", commitId, path, oldPath }, `${commitId.slice(0, 7)}-${name}.patch`),
    onRevertHunk: (hunk, stage) => change(true, hunk, stage),
    onApplyHunk: (hunk, stage) => change(false, hunk, stage),
  };
}

const ORDINALS = ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth"];

/** "first", "second", … for a parent's index; "11th" past ten (an octopus merge). */
function parentOrdinal(index: number): string {
  return ORDINALS[index] ?? `${index + 1}th`;
}

export function CommitDiffPane({
  repoPath,
  client,
  commitId,
  commits,
  focus,
  view,
  onViewChange,
  restoreBlockedReason,
  issueForge = null,
}: {
  repoPath: string;
  client: RepoClient;
  commitId: string;
  commits?: GraphCommit[];
  focus: FileHistoryFocus | null;
  view: CommitView;
  onViewChange: (view: CommitView) => void;
  restoreBlockedReason: string | null;
  // Passed to `CommitHeader` (#612).
  issueForge?: ForgeRepository | null;
}) {
  const {
    onSelectRow,
    onOpenFileHistory,
    onOpenLineHistory,
    onOpenFolderHistory,
    externalTools,
    onRestoreFileVersion,
    patchActions,
  } = useWorkspaceActions();
  const knownCommitIds = useMemo(() => new Set((commits ?? []).map((c) => c.id)), [commits]);
  // The loaded commits with this one as a parent (#618).
  const childIds = useMemo(
    () => (commits ?? []).filter((c) => c.parentIds.includes(commitId)).map((c) => c.id),
    [commits, commitId],
  );
  // A merge commit's diff is against its first parent unless another one is picked (#1009).
  const parentIds = useMemo(() => commits?.find((c) => c.id === commitId)?.parentIds ?? [], [commits, commitId]);
  const [parentPick, setParentPick] = useState<{ commitId: string; parentId: string } | null>(null);
  const comparedParent =
    parentPick !== null && parentPick.commitId === commitId && parentIds.includes(parentPick.parentId)
      ? parentPick.parentId
      : (parentIds[0] ?? null);
  // The picked parent when it isn't the first: the diff is then a range from it to the commit.
  const otherParent = comparedParent !== null && comparedParent !== parentIds[0] ? comparedParent : null;
  const parentPickerLabelId = useId();
  const fetchCommitDiff = (path: string, oldPath: string | null) => (opts: DiffOptions) =>
    otherParent !== null
      ? client.getRangeDiff(repoPath, otherParent, commitId, path, oldPath, opts)
      : oldPath != null
        ? client.getCommitRenameDiff(repoPath, commitId, oldPath, path, opts)
        : client.getCommitDiff(repoPath, commitId, path, opts);
  const diffSource = (oldPath: string | null): DiffSource =>
    otherParent !== null
      ? { kind: "Range", fromId: otherParent, toId: commitId, oldPath }
      : { kind: "Commit", commitId, oldPath };
  const [files, setFiles] = useState<ChangedFile[]>([]);
  const [error, setError] = useState<DescribedError | null>(null);
  const [collapsedPaths, setCollapsedPaths] = useState<Set<string>>(new Set());
  const [showAll, setShowAll] = useState(false);
  const [focusCollapsed, setFocusCollapsed] = useState(false);

  const loadFiles = () => {
    let ignore = false;
    (otherParent !== null
      ? client.getRangeFiles(repoPath, otherParent, commitId)
      : client.getCommitFiles(repoPath, commitId).then((paths) => paths.map((path) => ({ path, oldPath: null })))
    )
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
  }, [repoPath, client, commitId, otherParent]);

  // #620: the totals in the header and each file's +/−. `undefined` while counting, `null` for a
  // commit over the backend's cap (or a failed count): then the header has no totals.
  const [lineStats, setLineStats] = useState<{ commitId: string; stats: CommitStats | null } | null>(null);
  useEffect(() => {
    let ignore = false;
    Promise.resolve()
      .then(() => client.getCommitStats(repoPath, [commitId], true))
      .then((entries) => entries.find((entry) => entry.commitId === commitId)?.stats ?? null)
      .catch(() => null)
      .then((stats) => {
        if (!ignore) setLineStats({ commitId, stats });
      });
    return () => {
      ignore = true;
    };
  }, [repoPath, client, commitId]);
  const commitStats = lineStats?.commitId === commitId ? lineStats.stats : undefined;
  const fileStats = useMemo(
    () => new Map<string, FileStats>((commitStats?.files ?? []).map((file) => [file.path, file])),
    [commitStats],
  );

  const allCollapsed = files.length > 0 && files.every((file) => collapsedPaths.has(file.path));

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
    setCollapsedPaths(allCollapsed ? new Set() : new Set(files.map((file) => file.path)));
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

  const focused = focus !== null && !showAll;
  // Against another parent a rename is that parent's, from the range's file list.
  const focusOldPath =
    focus === null ? null : otherParent === null ? focus.oldPath : (files.find((f) => f.path === focus.path)?.oldPath ?? null);
  const diffKey = `commit:${commitId}:${otherParent ?? "first"}`;
  // Revert / Apply and the line counts are the first parent's, so another parent has neither.
  const patchFor = (path: string, oldPath: string | null) =>
    otherParent === null && patchActions ? commitFilePatch(patchActions, commitId, path, oldPath) : undefined;
  const statsFor = (path: string) => (otherParent === null ? fileStats.get(path) : undefined);

  return (
    <div>
      <CommitHeader
        key={commitId}
        repoPath={repoPath}
        client={client}
        commitId={commitId}
        commit={commits?.find((c) => c.id === commitId)}
        knownCommitIds={knownCommitIds}
        childIds={childIds}
        onSelectRow={onSelectRow}
        issueForge={issueForge}
        lineStats={commitStats}
      />
      <div className={treeStyles.viewToggle} role="group" aria-label="Commit files">
        <button type="button" aria-pressed={view === "changes"} onClick={() => onViewChange("changes")}>
          Changes
        </button>
        <button type="button" aria-pressed={view === "tree"} onClick={() => onViewChange("tree")}>
          All files
        </button>
      </div>
      {view !== "tree" && parentIds.length > 1 && comparedParent !== null && (
        <div className={styles.parentPicker}>
          <span id={parentPickerLabelId}>Changes compared with</span>
          <select
            aria-labelledby={parentPickerLabelId}
            value={comparedParent}
            onChange={(event) => setParentPick({ commitId, parentId: event.target.value })}
          >
            {parentIds.map((parentId, index) => (
              <option key={parentId} value={parentId}>
                {`${parentId.slice(0, 7)} (${parentOrdinal(index)} parent)`}
              </option>
            ))}
          </select>
        </div>
      )}
      {view !== "tree" && <DiffOptionsToolbar sticky />}
      {view === "tree" ? (
        <CommitTreeBrowser
          repoPath={repoPath}
          client={client}
          commitId={commitId}
          onSelectRow={onSelectRow}
          onOpenFileHistory={onOpenFileHistory}
          onOpenLineHistory={onOpenLineHistory}
          onOpenFolderHistory={onOpenFolderHistory}
          externalTools={externalTools}
          onRestoreFileVersion={onRestoreFileVersion}
          restoreBlockedReason={restoreBlockedReason}
        />
      ) : focused ? (
        <>
          <ul className={styles.fileList}>
            <ReadOnlyFileSection
              key={focus.path}
              repoPath={repoPath}
              client={client}
              path={focus.path}
              oldPath={focusOldPath}
              diffKey={diffKey}
              fetchDiff={fetchCommitDiff(focus.path, focusOldPath)}
              blameAt={commitId}
              collapsed={focusCollapsed}
              onToggleCollapse={() => setFocusCollapsed((c) => !c)}
              onSelectRow={onSelectRow}
              onOpenLineHistory={onOpenLineHistory}
              onOpenFolderHistory={onOpenFolderHistory}
              externalTools={externalTools}
              toolSource={diffSource(focusOldPath)}
              previewSource={diffSource(focusOldPath)}
              patch={patchFor(focus.path, focusOldPath)}
              lineStats={statsFor(focus.path)}
            />
          </ul>
          {files.length > 1 && (
            <button type="button" onClick={() => setShowAll(true)}>
              Show all {files.length} files
            </button>
          )}
        </>
      ) : (
        <>
          {files.length > 0 && (
            <div className={styles.groupHeading}>
              <CollapseAllToggle allCollapsed={allCollapsed} onToggle={toggleCollapseAll} />
            </div>
          )}
          <ul className={styles.fileList}>
            {files.map(({ path, oldPath }) => (
              <ReadOnlyFileSection
                key={`${oldPath ?? ""}→${path}`}
                repoPath={repoPath}
                client={client}
                path={path}
                oldPath={oldPath}
                diffKey={diffKey}
                fetchDiff={fetchCommitDiff(path, oldPath)}
                blameAt={commitId}
                collapsed={collapsedPaths.has(path)}
                onToggleCollapse={() => toggleCollapse(path)}
                onSelectRow={onSelectRow}
                onOpenFileHistory={onOpenFileHistory}
                onOpenLineHistory={onOpenLineHistory}
                onOpenFolderHistory={onOpenFolderHistory}
                externalTools={externalTools}
                toolSource={diffSource(oldPath)}
                previewSource={diffSource(oldPath)}
                patch={patchFor(path, oldPath)}
                lineStats={statsFor(path)}
              />
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
