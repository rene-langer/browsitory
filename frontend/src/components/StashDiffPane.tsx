import { withBlockedReason, type StashActions } from "./diffPaneShared";
import { describeError, type DescribedError } from "../lib/errorMessages";
import { useEffect, useState } from "react";
import type { RepoClient, StashEntry, StashFiles } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./DiffPane.module.css";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { InlineError } from "./primitives/InlineError";
import { ReadOnlyFileSection } from "./ReadOnlyFileSection";
import { stashActions as stashActionRules } from "../lib/actions/stashActions";

/**
 * A selected stash: its message, `stash@{n}`, Apply / Pop / Drop… with "Restore staged changes",
 * then its tracked changes (diffed against the HEAD it was made on) and its untracked files
 * (diffed as whole-file adds through the parentless untracked-files commit).
 */
export function StashDiffPane({
  repoPath,
  client,
  stash,
  actions,
  onSelectRow,
}: {
  repoPath: string;
  client: RepoClient;
  stash: StashEntry;
  actions: StashActions;
  onSelectRow: (row: SelectedRow) => void;
}) {
  const [files, setFiles] = useState<StashFiles | null>(null);
  const [error, setError] = useState<DescribedError | null>(null);
  const [collapsedPaths, setCollapsedPaths] = useState<Set<string>>(new Set());
  const [confirmingDrop, setConfirmingDrop] = useState(false);

  const loadFiles = () => {
    let ignore = false;
    client
      .getStashFiles(repoPath, stash.commitId)
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
  }, [repoPath, client, stash.commitId]);

  const toggleCollapse = (key: string) => {
    setCollapsedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const rules = stashActionRules(actions.blockers, stash);
  const dropDisabledReason = actions.blockers.drop;
  const untrackedCommitId = files?.untrackedCommitId ?? null;

  return (
    <section aria-label="Stash details" className={styles.stashDetails}>
      <div className={styles.stashHeader}>
        <h2 className={styles.stashMessage}>{stash.message}</h2>
        <span className={styles.stashRef}>
          stash@{"{"}
          {stash.index}
          {"}"} · {stash.commitId.slice(0, 7)}
        </span>
        <Toolbar aria-label="Stash actions">
          <button
            type="button"
            disabled={!rules.apply.enabled}
            title={rules.apply.reason}
            onClick={() => actions.onApply(stash.index)}
          >
            {rules.apply.label}
          </button>
          <button
            type="button"
            disabled={!rules.pop.enabled}
            title={rules.pop.reason}
            onClick={() => actions.onPop(stash.index)}
          >
            {rules.pop.label}
          </button>
          <button
            type="button"
            disabled={!rules.drop.enabled}
            title={rules.drop.reason}
            onClick={() => setConfirmingDrop(true)}
          >
            {rules.drop.label}
          </button>
          <label className={styles.restoreIndex}>
            <input
              type="checkbox"
              checked={actions.restoreIndex}
              onChange={(event) => actions.onSetRestoreIndex(event.target.checked)}
            />
            Restore staged changes
          </label>
        </Toolbar>
      </div>
      {error !== null ? (
        <InlineError message={error.message} hint={error.hint} onDismiss={() => setError(null)} onRetry={loadFiles} />
      ) : files === null ? null : files.tracked.length === 0 && files.untracked.length === 0 ? (
        <p className={styles.stashEmpty}>This stash has no file changes.</p>
      ) : (
        <>
          {files.tracked.length > 0 && (
            <>
              <div className={styles.groupHeading}>
                <span>Changes ({files.tracked.length})</span>
              </div>
              <ul className={styles.fileList}>
                {files.tracked.map((path) => (
                  <ReadOnlyFileSection
                    key={path}
                    repoPath={repoPath}
                    client={client}
                    path={path}
                    diffKey={`commit:${stash.commitId}`}
                    fetchDiff={(opts) => client.getCommitDiff(repoPath, stash.commitId, path, opts)}
                    previewSource={{ kind: "Commit", commitId: stash.commitId, oldPath: null }}
                    blameAt={stash.commitId}
                    collapsed={collapsedPaths.has(`t:${path}`)}
                    onToggleCollapse={() => toggleCollapse(`t:${path}`)}
                    onSelectRow={onSelectRow}
                  />
                ))}
              </ul>
            </>
          )}
          {files.untracked.length > 0 && untrackedCommitId !== null && (
            <>
              <div className={styles.groupHeading}>
                <span>Untracked ({files.untracked.length})</span>
              </div>
              <ul className={styles.fileList}>
                {files.untracked.map((path) => (
                  <ReadOnlyFileSection
                    key={path}
                    repoPath={repoPath}
                    client={client}
                    path={path}
                    diffKey={`commit:${untrackedCommitId}`}
                    fetchDiff={(opts) => client.getCommitDiff(repoPath, untrackedCommitId, path, opts)}
                    previewSource={{ kind: "Commit", commitId: untrackedCommitId, oldPath: null }}
                    blameAt={untrackedCommitId}
                    collapsed={collapsedPaths.has(`u:${path}`)}
                    onToggleCollapse={() => toggleCollapse(`u:${path}`)}
                    onSelectRow={onSelectRow}
                  />
                ))}
              </ul>
            </>
          )}
        </>
      )}
      {confirmingDrop && (
        <ConfirmDialog
          ariaLabel={`Drop stash ${stash.message}`}
          message={withBlockedReason(<p>Drop "{stash.message}"? This cannot be undone.</p>, dropDisabledReason)}
          confirmLabel="Drop stash"
          confirmDisabled={dropDisabledReason !== null}
          confirmTitle={dropDisabledReason ?? undefined}
          onConfirm={() => {
            if (dropDisabledReason !== null) return;
            setConfirmingDrop(false);
            actions.onDrop(stash.index);
          }}
          onCancel={() => setConfirmingDrop(false)}
        />
      )}
    </section>
  );
}
