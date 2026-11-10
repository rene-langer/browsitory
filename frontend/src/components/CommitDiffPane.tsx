import type { CommitView, DiffPanePatchActions, FileHistoryFocus } from "./diffPaneShared";
import { describeError, type DescribedError } from "../lib/errorMessages";
import { useEffect, useMemo, useState } from "react";
import type { CommitHunk, DiffOptions, ForgeRepository, GraphCommit, RepoClient } from "../ipc/RepoClient";
import { useWorkspaceActions } from "../state/useWorkspaceActions";
import { CollapseAllToggle } from "./CollapseToggle";
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
  const change = (reverse: boolean, hunk: CommitHunk | null) =>
    actions.onApplyCommitChange({ commitId, path, oldPath, hunk, reverse, stage: false });
  const name = path.split("/").pop() ?? path;
  return {
    blockedReason: actions.blockedReason,
    onRevertFile: () => change(true, null),
    onApplyFile: () => change(false, null),
    onSavePatch: () =>
      actions.onSavePatch({ kind: "CommitFile", commitId, path, oldPath }, `${commitId.slice(0, 7)}-${name}.patch`),
    onRevertHunk: (hunk) => change(true, hunk),
    onApplyHunk: (hunk) => change(false, hunk),
  };
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
  const fetchCommitDiff = (path: string, oldPath: string | null) => (opts: DiffOptions) =>
    oldPath != null
      ? client.getCommitRenameDiff(repoPath, commitId, oldPath, path, opts)
      : client.getCommitDiff(repoPath, commitId, path, opts);
  const [files, setFiles] = useState<string[]>([]);
  const [error, setError] = useState<DescribedError | null>(null);
  const [collapsedPaths, setCollapsedPaths] = useState<Set<string>>(new Set());
  const [showAll, setShowAll] = useState(false);
  const [focusCollapsed, setFocusCollapsed] = useState(false);

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

  const focused = focus !== null && !showAll;

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
      />
      <div className={treeStyles.viewToggle} role="group" aria-label="Commit files">
        <button type="button" aria-pressed={view === "changes"} onClick={() => onViewChange("changes")}>
          Changes
        </button>
        <button type="button" aria-pressed={view === "tree"} onClick={() => onViewChange("tree")}>
          All files
        </button>
      </div>
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
              oldPath={focus.oldPath}
              diffKey={`commit:${commitId}`}
              fetchDiff={fetchCommitDiff(focus.path, focus.oldPath)}
              blameAt={commitId}
              collapsed={focusCollapsed}
              onToggleCollapse={() => setFocusCollapsed((c) => !c)}
              onSelectRow={onSelectRow}
              onOpenLineHistory={onOpenLineHistory}
              onOpenFolderHistory={onOpenFolderHistory}
              externalTools={externalTools}
              toolSource={{ kind: "Commit", commitId, oldPath: focus.oldPath }}
              previewSource={{ kind: "Commit", commitId, oldPath: focus.oldPath }}
              patch={patchActions && commitFilePatch(patchActions, commitId, focus.path, focus.oldPath)}
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
            {files.map((path) => (
              <ReadOnlyFileSection
                key={path}
                repoPath={repoPath}
                client={client}
                path={path}
                diffKey={`commit:${commitId}`}
                fetchDiff={fetchCommitDiff(path, null)}
                blameAt={commitId}
                collapsed={collapsedPaths.has(path)}
                onToggleCollapse={() => toggleCollapse(path)}
                onSelectRow={onSelectRow}
                onOpenFileHistory={onOpenFileHistory}
                onOpenLineHistory={onOpenLineHistory}
                onOpenFolderHistory={onOpenFolderHistory}
                externalTools={externalTools}
                toolSource={{ kind: "Commit", commitId, oldPath: null }}
                previewSource={{ kind: "Commit", commitId, oldPath: null }}
                patch={patchActions && commitFilePatch(patchActions, commitId, path, null)}
              />
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
