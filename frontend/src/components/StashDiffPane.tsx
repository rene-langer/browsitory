import type { StashActions } from "./diffPaneShared";
import { describeError, type DescribedError } from "../lib/errorMessages";
import { useEffect, useState } from "react";
import type { RepoClient, StashEntry, StashFiles } from "../ipc/RepoClient";
import { useWorkspaceActions } from "../state/useWorkspaceActions";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./DiffPane.module.css";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ReasonButton } from "./primitives/ReasonButton";
import { disabledReason } from "../lib/actions/action";
import { InlineError } from "./primitives/InlineError";
import { ReadOnlyFileSection } from "./ReadOnlyFileSection";
import { DiffOptionsToolbar } from "./DiffOptionsToolbar";
import { stashActions as stashActionRules, stashDetails, stashTitle } from "../lib/actions/stashActions";

/**
 * A selected stash: its name, `stash@{n}` with "On main · 2 days ago · 3 files", Apply / Pop /
 * Drop… (and "Apply as unstaged" / "Pop as unstaged" for a stash that recorded staged changes,
 * #657), then its tracked changes (diffed against the HEAD it was made on) and its untracked files
 * (diffed as whole-file adds through the parentless untracked-files commit). Each file's menu
 * opens it in the editor or file manager, or in the diff tool against the same commit (#129).
 */
export function StashDiffPane({
  repoPath,
  client,
  stash,
  actions,
}: {
  repoPath: string;
  client: RepoClient;
  stash: StashEntry;
  actions: StashActions;
}) {
  const { onSelectRow, externalTools } = useWorkspaceActions();
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
        <h2 className={styles.stashMessage} title={stash.message}>
          {stashTitle(stash)}
        </h2>
        <span className={styles.stashRef}>
          stash@{"{"}
          {stash.index}
          {"}"} · {stash.commitId.slice(0, 7)} · {stashDetails(stash)}
        </span>
        <Toolbar aria-label="Stash actions">
          <ReasonButton reason={disabledReason(rules.apply)} onClick={() => actions.onApply(stash.index, true)}>
            {rules.apply.label}
          </ReasonButton>
          {rules.applyUnstaged !== null && (
            <ReasonButton
              reason={disabledReason(rules.applyUnstaged)}
              title="Apply, leaving its staged changes unstaged"
              onClick={() => actions.onApply(stash.index, false)}
            >
              {rules.applyUnstaged.label}
            </ReasonButton>
          )}
          <ReasonButton reason={disabledReason(rules.pop)} onClick={() => actions.onPop(stash.index, true)}>
            {rules.pop.label}
          </ReasonButton>
          {rules.popUnstaged !== null && (
            <ReasonButton
              reason={disabledReason(rules.popUnstaged)}
              title="Pop, leaving its staged changes unstaged"
              onClick={() => actions.onPop(stash.index, false)}
            >
              {rules.popUnstaged.label}
            </ReasonButton>
          )}
          <ReasonButton reason={disabledReason(rules.drop)} onClick={() => setConfirmingDrop(true)}>
            {rules.drop.label}
          </ReasonButton>
        </Toolbar>
      </div>
      {error !== null ? (
        <InlineError message={error.message} hint={error.hint} onDismiss={() => setError(null)} onRetry={loadFiles} />
      ) : files === null ? null : files.tracked.length === 0 && files.untracked.length === 0 ? (
        <p className={styles.stashEmpty}>This stash has no file changes.</p>
      ) : (
        <>
          <DiffOptionsToolbar sticky />
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
                    externalTools={externalTools}
                    toolSource={{ kind: "Commit", commitId: stash.commitId, oldPath: null }}
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
                    externalTools={externalTools}
                    toolSource={{ kind: "Commit", commitId: untrackedCommitId, oldPath: null }}
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
          ariaLabel={`Drop stash ${stashTitle(stash)}`}
          message={<p>Drop "{stashTitle(stash)}"? This cannot be undone.</p>}
          confirmLabel="Drop stash"
          // #521: the repository can become busy while the confirmation is open.
          confirmReason={dropDisabledReason}
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
