import { useRef, type KeyboardEvent } from "react";
import { X, Plus } from "lucide-react";
import type { OpenRepo } from "../state/useOpenRepos";
import { repoPanelId, repoTabId } from "./repoTabIds";
import styles from "./RepoTabs.module.css";

interface TabGroup {
  workspaceId: string | null;
  workspaceName: string | null;
  repos: OpenRepo[];
}

function groupContiguousTabs(openRepos: OpenRepo[], workspaceNames: Record<string, string>): TabGroup[] {
  const groups: TabGroup[] = [];
  for (const repo of openRepos) {
    const workspaceName = repo.workspaceId !== null ? workspaceNames[repo.workspaceId] ?? null : null;
    const last = groups[groups.length - 1];
    if (last !== undefined && last.workspaceId === repo.workspaceId && workspaceName !== null) {
      last.repos.push(repo);
      continue;
    }
    groups.push({ workspaceId: workspaceName !== null ? repo.workspaceId : null, workspaceName, repos: [repo] });
  }
  return groups;
}

export function RepoTabs({
  openRepos,
  activePath,
  busyPaths,
  workspaceNames,
  onSwitchTo,
  onClose,
  onCloseGroup,
  onAddTab,
}: {
  openRepos: OpenRepo[];
  activePath: string | null;
  busyPaths: ReadonlySet<string>;
  workspaceNames: Record<string, string>;
  onSwitchTo: (path: string) => void;
  onClose: (path: string) => void;
  onCloseGroup: (paths: string[]) => void;
  onAddTab: () => void;
}) {
  const tablistRef = useRef<HTMLDivElement>(null);
  if (openRepos.length === 0) return null;

  // Roving tabindex: only the selected tab (or the first, when none is) is a tab stop; arrow keys
  // move between tabs and activate them (automatic activation, WAI-ARIA APG tabs pattern).
  const tabStopPath = openRepos.some((repo) => repo.path === activePath) ? activePath : openRepos[0].path;

  const handleTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>, path: string, busy: boolean) => {
    const tabs = Array.from(tablistRef.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]') ?? []);
    const current = tabs.indexOf(event.currentTarget);
    if (current === -1) return;
    // WAI-ARIA APG tabs pattern: Delete closes the focused tab — any tab, not just the active one.
    // This is the host-independent keyboard path for the mouse-only close button below; Ctrl/Cmd+W
    // (App.tsx) can be swallowed by the Tauri/VSCode host first. Focus moves to the tab that takes
    // this one's place (the next, else the previous) — the same neighbour `useOpenRepos.closeRepo`
    // activates when the closed tab was the active one — so it isn't dropped to <body>.
    if (event.key === "Delete") {
      event.preventDefault();
      if (busy) return;
      (tabs[current + 1] ?? tabs[current - 1])?.focus();
      onClose(path);
      return;
    }
    const keys = ["ArrowLeft", "ArrowRight", "Home", "End"];
    if (!keys.includes(event.key)) return;
    event.preventDefault();
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? tabs.length - 1
          : (current + (event.key === "ArrowRight" ? 1 : -1) + tabs.length) % tabs.length;
    tabs[next].focus();
    tabs[next].click();
  };

  const renderTab = (repo: OpenRepo) => {
    const selected = repo.path === activePath;
    const busy = busyPaths.has(repo.path);
    return (
      <div key={repo.path} role="presentation" className={selected ? `${styles.tab} ${styles.active}` : styles.tab}>
        <button
          type="button"
          role="tab"
          id={repoTabId(repo.path)}
          aria-selected={selected}
          aria-controls={repoPanelId(repo.path)}
          tabIndex={repo.path === tabStopPath ? 0 : -1}
          aria-keyshortcuts="Delete"
          onKeyDown={(event) => handleTabKeyDown(event, repo.path, busy)}
          title={repo.path}
          className={styles.tabLabel}
          onClick={() => onSwitchTo(repo.path)}
        >
          {repo.displayName}
        </button>
        {/* `aria-hidden`/`tabIndex={-1}` pull this out of the tablist's accessible children (a
            tablist may only own `role="tab"` elements per WAI-ARIA — axe's `aria-required-children`
            flags a focusable close button here) and out of the Tab order; it stays clickable by
            mouse. Its keyboard equivalents are Delete on the focused tab (above), the palette's
            "Close tab", and Ctrl/Cmd+W (App.tsx) where the host doesn't claim that key first. */}
        <button
          type="button"
          className={styles.closeButton}
          aria-hidden="true"
          tabIndex={-1}
          title={busy ? "This repo has an operation in progress" : "Close (Delete)"}
          disabled={busy}
          onClick={() => onClose(repo.path)}
        >
          <X size={12} aria-hidden="true" />
        </button>
      </div>
    );
  };

  return (
    <div className={styles.strip}>
    <div ref={tablistRef} className={styles.tabs} role="tablist" aria-label="Open repositories">
      {groupContiguousTabs(openRepos, workspaceNames).map((group, index) =>
        group.workspaceName !== null ? (
          <div key={`group-${index}`} role="presentation" className={styles.group}>
            <div role="presentation" className={styles.groupHeader}>
              <span className={styles.groupLabel}>{group.workspaceName}</span>
              <button
                type="button"
                className={styles.closeButton}
                aria-hidden="true"
                tabIndex={-1}
                aria-label={`Close ${group.workspaceName}`}
                title={group.repos.some((repo) => busyPaths.has(repo.path)) ? "A repo in this workspace has an operation in progress" : undefined}
                disabled={group.repos.some((repo) => busyPaths.has(repo.path))}
                onClick={() => onCloseGroup(group.repos.map((repo) => repo.path))}
              >
                <X size={12} aria-hidden="true" />
              </button>
            </div>
            <div role="presentation" className={styles.groupTabs}>{group.repos.map(renderTab)}</div>
          </div>
        ) : (
          renderTab(group.repos[0])
        ),
      )}
    </div>
      {/* Outside the scrolling tablist so it stays reachable when tabs overflow. */}
      <button type="button" className={styles.addButton} aria-label="Open another repository" title="Open another repository" onClick={onAddTab}>
        <Plus size={14} aria-hidden="true" />
      </button>
    </div>
  );
}
