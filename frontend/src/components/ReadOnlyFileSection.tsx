import { useEffect, useState } from "react";
import type { BlameLine, DiffHunk, DiffOptions, RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import type { SelectedRow } from "../state/useAppState";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { BlameView } from "./BlameView";
import { CollapseToggle } from "./CollapseToggle";
import styles from "./DiffPane.module.css";
import { DiffView } from "./DiffView";
import { InlineError } from "./primitives/InlineError";

/**
 * One file's diff in a read-only view (a commit or a compare), with Blame and History.
 * `fetchDiff` runs again whenever `diffKey`, the path or the diff settings change.
 */
export function ReadOnlyFileSection({
  repoPath,
  client,
  path,
  oldPath,
  diffKey,
  fetchDiff,
  blameAt,
  collapsed,
  onToggleCollapse,
  onSelectRow,
  onOpenFileHistory,
}: {
  repoPath: string;
  client: RepoClient;
  path: string;
  oldPath?: string | null;
  /** Identifies the diff source; a change refetches. */
  diffKey: string;
  fetchDiff: (opts: DiffOptions) => Promise<DiffHunk[]>;
  /** Revision Blame runs at (a commit id or "HEAD"). */
  blameAt: string;
  collapsed: boolean;
  onToggleCollapse: () => void;
  onSelectRow: (row: SelectedRow) => void;
  onOpenFileHistory?: (path: string) => void;
}) {
  const [mode, setMode] = useState<"diff" | "blame">("diff");
  const [hunks, setHunks] = useState<DiffHunk[] | null>(null);
  const [blameLines, setBlameLines] = useState<BlameLine[]>([]);
  const [error, setError] = useState<DescribedError | null>(null);

  const { settings } = useDiffSettingsContext();
  const diffOptions = { ignoreWhitespace: settings.ignoreWhitespace, contextLines: settings.contextLines };
  const loadDiff = () => {
    let ignore = false;
    const request = fetchDiff(diffOptions);
    request
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
  }, [diffKey, path, oldPath, mode, collapsed, settings.ignoreWhitespace, settings.contextLines]);

  const loadBlame = () => {
    let ignore = false;
    client
      .getBlame(repoPath, blameAt, path)
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
  }, [repoPath, client, blameAt, path, mode]);

  return (
    <li className={styles.fileSection}>
      <div className={styles.fileSectionHeader}>
        <CollapseToggle collapsed={collapsed} path={path} onToggle={onToggleCollapse} />
        <span className={styles.path}>{path}</span>
        {oldPath != null && <span className={styles.renamedFrom}>Renamed from {oldPath}</span>}
        <div className={styles.rowActions}>
          <button type="button" onClick={() => setMode("blame")}>
            Blame
          </button>
          {onOpenFileHistory !== undefined && (
            <button type="button" aria-label={`History of ${path}`} onClick={() => onOpenFileHistory(path)}>
              History
            </button>
          )}
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
            <DiffView hunks={hunks} path={path} />
          )}
        </div>
      )}
    </li>
  );
}
