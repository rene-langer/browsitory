import { useEffect, useRef, useState } from "react";
import type { ChangedFile, DiffOptions, GraphCommit, RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import type { SelectedRow } from "../state/useAppState";
import type { CompareState } from "../state/useCompare";
import type { ExternalToolsApi } from "../state/useExternalTools";
import { CollapseAllToggle } from "./CollapseToggle";
import styles from "./CompareDiffPane.module.css";
import diffStyles from "./DiffPane.module.css";
import { InlineError } from "./primitives/InlineError";
import { ReadOnlyFileSection } from "./ReadOnlyFileSection";

/** Above this many files every section starts collapsed: each open section fetches its own diff. */
export const COMPARE_EXPAND_LIMIT = 50;

export function CompareDiffPane({
  repoPath,
  client,
  compare,
  commits,
  refreshGeneration,
  onSwap,
  onClose,
  onRetry,
  onSelectRow,
  onOpenFileHistory,
  externalTools,
}: {
  repoPath: string;
  client: RepoClient;
  compare: CompareState;
  commits: GraphCommit[];
  refreshGeneration: number;
  onSwap: () => void;
  onClose: () => void;
  onRetry: () => void;
  onSelectRow: (row: SelectedRow) => void;
  onOpenFileHistory?: (path: string) => void;
  externalTools?: ExternalToolsApi;
}) {
  const { target, range } = compare;
  const fromId = range?.fromId ?? null;
  const toId = range?.toId ?? null;
  // Commits never change; the working tree does.
  const reloadKey = target.kind === "workingTree" ? refreshGeneration : 0;
  const rangeKey = fromId === null ? null : `${fromId}..${toId ?? "worktree"}`;

  const [files, setFiles] = useState<ChangedFile[] | null>(null);
  const [error, setError] = useState<DescribedError | null>(null);
  const [collapsedPaths, setCollapsedPaths] = useState<Set<string>>(new Set());
  const [retryToken, setRetryToken] = useState(0);
  const collapseInitializedFor = useRef<string | null>(null);

  // A new range (swap, a moved branch) drops the old list in the same render, so no section
  // diffs an old path against the new ends. Adjusted during render, not in an effect:
  // `react-hooks/set-state-in-effect` forbids a synchronous setState there.
  const [shownFor, setShownFor] = useState(rangeKey);
  if (shownFor !== rangeKey) {
    setShownFor(rangeKey);
    setFiles(null);
    setError(null);
  }

  useEffect(() => {
    if (fromId === null) return;
    let ignore = false;
    const request =
      toId === null ? client.getWorkingTreeCompareFiles(repoPath, fromId) : client.getRangeFiles(repoPath, fromId, toId);
    request.then(
      (next) => {
        if (ignore) return;
        setFiles(next);
        setError(null);
        if (collapseInitializedFor.current !== rangeKey) {
          collapseInitializedFor.current = rangeKey;
          setCollapsedPaths(next.length > COMPARE_EXPAND_LIMIT ? new Set(next.map((f) => f.path)) : new Set());
        }
      },
      (err: unknown) => {
        if (!ignore) setError(describeError(err));
      },
    );
    return () => {
      ignore = true;
    };
  }, [repoPath, client, fromId, toId, rangeKey, reloadKey, retryToken]);

  const short = (id: string) => commits.find((c) => c.id === id)?.shortId ?? id.slice(0, 7);
  const described = (id: string) => {
    const found = commits.find((c) => c.id === id);
    return found === undefined ? id.slice(0, 7) : `${found.shortId} ${found.summary}`;
  };

  let heading: string;
  if (target.kind === "commits") heading = `Comparing ${short(target.fromId)}..${short(target.toId)}`;
  else if (target.kind === "branch") heading = `Changes on ${target.label} since it diverged`;
  else heading = `Comparing ${short(target.commitId)} with the working tree`;

  const toggleCollapse = (path: string) =>
    setCollapsedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  const allCollapsed = files !== null && files.length > 0 && files.every((f) => collapsedPaths.has(f.path));

  const fetchFor = (file: ChangedFile) => (opts: DiffOptions) =>
    toId === null
      ? client.getWorkingTreeCompareDiff(repoPath, fromId as string, file.path, file.oldPath, opts)
      : client.getRangeDiff(repoPath, fromId as string, toId, file.path, file.oldPath, opts);

  let body;
  if (compare.error !== null) {
    body = <InlineError message={compare.error.message} hint={compare.error.hint} onRetry={onRetry} onDismiss={onClose} />;
  } else if (fromId === null) {
    body = <p className={styles.status}>{`Finding where ${target.kind === "branch" ? target.label : "it"} diverged…`}</p>;
  } else if (error !== null) {
    body = <InlineError message={error.message} hint={error.hint} onRetry={() => setRetryToken((token) => token + 1)} onDismiss={onClose} />;
  } else if (files === null) {
    body = null;
  } else if (files.length === 0) {
    body = <p className={styles.status}>No differences.</p>;
  } else {
    body = (
      <>
        <div className={diffStyles.groupHeading}>
          <span className={styles.status}>{files.length === 1 ? "1 file changed" : `${files.length} files changed`}</span>
          <CollapseAllToggle
            allCollapsed={allCollapsed}
            onToggle={() => setCollapsedPaths(allCollapsed ? new Set() : new Set(files.map((f) => f.path)))}
          />
        </div>
        <ul className={diffStyles.fileList}>
          {files.map((file) => (
            <ReadOnlyFileSection
              key={`${file.oldPath ?? ""}→${file.path}`}
              repoPath={repoPath}
              client={client}
              path={file.path}
              oldPath={file.oldPath}
              diffKey={`${rangeKey}:${reloadKey}`}
              fetchDiff={fetchFor(file)}
              blameAt={toId ?? "HEAD"}
              collapsed={collapsedPaths.has(file.path)}
              onToggleCollapse={() => toggleCollapse(file.path)}
              onSelectRow={onSelectRow}
              onOpenFileHistory={onOpenFileHistory}
              externalTools={externalTools}
              toolSource={{ kind: "Range", fromId: fromId as string, toId, oldPath: file.oldPath }}
              previewSource={{ kind: "Range", fromId: fromId as string, toId, oldPath: file.oldPath }}
            />
          ))}
        </ul>
      </>
    );
  }

  return (
    <div>
      <section className={styles.header} aria-label="Comparison">
        <div className={styles.titleRow}>
          <h2 className={styles.title}>{heading}</h2>
          {target.kind === "commits" && (
            <button type="button" aria-label="Swap sides" onClick={onSwap}>
              Swap
            </button>
          )}
          <button type="button" aria-label="Close compare" onClick={onClose}>
            ×
          </button>
        </div>
        {fromId !== null && (
          <dl className={styles.ends}>
            <dt>From</dt>
            <dd>{target.kind === "branch" ? `${short(fromId)} (merge base)` : described(fromId)}</dd>
            <dt>To</dt>
            <dd>
              {toId === null ? "Working tree" : target.kind === "branch" ? `${short(toId)} ${target.label}` : described(toId)}
            </dd>
          </dl>
        )}
      </section>
      {body}
    </div>
  );
}
