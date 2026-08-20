import { X, Plus } from "lucide-react";
import type { OpenRepo } from "../state/useOpenRepos";
import styles from "./RepoTabs.module.css";

export function RepoTabs({
  openRepos,
  activePath,
  busyPaths,
  onSwitchTo,
  onClose,
  onAddTab,
}: {
  openRepos: OpenRepo[];
  activePath: string | null;
  busyPaths: ReadonlySet<string>;
  onSwitchTo: (path: string) => void;
  onClose: (path: string) => void;
  onAddTab: () => void;
}) {
  if (openRepos.length === 0) return null;

  return (
    <div className={styles.tabs} role="tablist" aria-label="Open repositories">
      {openRepos.map((repo) => {
        const selected = repo.path === activePath;
        const busy = busyPaths.has(repo.path);
        return (
          <div key={repo.path} className={selected ? `${styles.tab} ${styles.active}` : styles.tab}>
            <button
              type="button"
              role="tab"
              aria-selected={selected}
              title={repo.path}
              className={styles.tabLabel}
              onClick={() => onSwitchTo(repo.path)}
            >
              {repo.displayName}
            </button>
            <button
              type="button"
              className={styles.closeButton}
              aria-label={`Close ${repo.displayName}`}
              title={busy ? "This repo has an operation in progress" : undefined}
              disabled={busy}
              onClick={() => onClose(repo.path)}
            >
              <X size={12} aria-hidden="true" />
            </button>
          </div>
        );
      })}
      <button type="button" className={styles.addButton} aria-label="Open another repository" onClick={onAddTab}>
        <Plus size={14} aria-hidden="true" />
      </button>
    </div>
  );
}
