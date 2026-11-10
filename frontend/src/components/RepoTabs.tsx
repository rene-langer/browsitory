import { useRef, useState, type KeyboardEvent } from "react";
import { ChevronDown, X, Plus } from "lucide-react";
import type { OpenRepo } from "../state/useOpenRepos";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import { isMenuKey, menuAnchor, rectAnchor } from "../lib/menuKeys";
import { repoPanelId, repoTabId } from "./repoTabIds";
import { TabList } from "./TabList";
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
  tabMenuItems,
  onTabMenuOpen,
  changedPaths,
  onRenameTab,
}: {
  openRepos: OpenRepo[];
  activePath: string | null;
  busyPaths: ReadonlySet<string>;
  workspaceNames: Record<string, string>;
  onSwitchTo: (path: string) => void;
  onClose: (path: string) => void;
  onCloseGroup: (paths: string[]) => void;
  onAddTab: () => void;
  /** A tab's context menu (right-click, the ContextMenu key or Shift+F10); omitted, none. */
  tabMenuItems?: (path: string) => ContextMenuItem[];
  /** Called as a tab's context menu opens, so items that depend on the disk can be read again. */
  onTabMenuOpen?: (path: string) => void;
  /** Repositories with uncommitted changes: their tabs show a marker (#613). */
  changedPaths?: ReadonlySet<string>;
  /** Opens the rename dialog of a tab (F2 on the focused tab, #625); omitted, F2 does nothing. */
  onRenameTab?: (path: string) => void;
}) {
  const tablistRef = useRef<HTMLDivElement>(null);
  const listButtonRef = useRef<HTMLButtonElement>(null);
  const [menu, setMenu] = useState<{ path: string; x: number; y: number } | null>(null);
  const [tabList, setTabList] = useState<{ x: number; y: number } | null>(null);
  if (openRepos.length === 0) return null;

  // Roving tabindex: only the selected tab (or the first, when none is) is a tab stop; arrow keys
  // move between tabs and activate them (automatic activation, WAI-ARIA APG tabs pattern).
  const tabStopPath = openRepos.some((repo) => repo.path === activePath) ? activePath : openRepos[0].path;

  const handleTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>, path: string, busy: boolean) => {
    const tabs = Array.from(tablistRef.current?.querySelectorAll<HTMLButtonElement>('[role="tab"]') ?? []);
    const current = tabs.indexOf(event.currentTarget);
    if (current === -1) return;
    if (tabMenuItems !== undefined && isMenuKey(event)) {
      event.preventDefault();
      setMenu({ path, ...rectAnchor(event.currentTarget) });
      onTabMenuOpen?.(path);
      return;
    }
    // WAI-ARIA APG tabs pattern: Delete closes the focused tab — any tab, not just the active one.
    // This is the host-independent keyboard path for the mouse-only close button below; Ctrl/Cmd+W
    // (`useWorkspaceShortcuts.ts`) can be swallowed by the Tauri/VSCode host first. Focus moves to the tab that takes
    // this one's place (the next, else the previous) — the same neighbour `useOpenRepos.closeRepo`
    // activates when the closed tab was the active one — so it isn't dropped to <body>.
    if (event.key === "F2" && onRenameTab !== undefined) {
      event.preventDefault();
      onRenameTab(path);
      return;
    }
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
    const changed = changedPaths?.has(repo.path) ?? false;
    const changesId = `${repoTabId(repo.path)}-changes`;
    return (
      <div key={repo.path} role="presentation" className={selected ? `${styles.tab} ${styles.active}` : styles.tab}>
        <button
          type="button"
          role="tab"
          id={repoTabId(repo.path)}
          aria-selected={selected}
          aria-controls={repoPanelId(repo.path)}
          tabIndex={repo.path === tabStopPath ? 0 : -1}
          aria-keyshortcuts={onRenameTab !== undefined ? "Delete F2" : "Delete"}
          aria-describedby={changed ? changesId : undefined}
          aria-haspopup={tabMenuItems !== undefined ? "menu" : undefined}
          onKeyDown={(event) => handleTabKeyDown(event, repo.path, busy)}
          onContextMenu={
            tabMenuItems === undefined
              ? undefined
              : (event) => {
                  event.preventDefault();
                  setMenu({ path: repo.path, ...menuAnchor(event, event.currentTarget) });
                  onTabMenuOpen?.(repo.path);
                }
          }
          title={repo.path}
          className={styles.tabLabel}
          onClick={() => onSwitchTo(repo.path)}
        >
          {repo.displayName}
        </button>
        {/* The dot is decoration; the tab's description carries the meaning (not only colour). */}
        {changed && (
          <>
            <span className={styles.changeMarker} aria-hidden="true" title="Uncommitted changes" />
            <span id={changesId} hidden>
              Uncommitted changes
            </span>
          </>
        )}
        {/* `aria-hidden`/`tabIndex={-1}` pull this out of the tablist's accessible children (a
            tablist may only own `role="tab"` elements per WAI-ARIA — axe's `aria-required-children`
            flags a focusable close button here) and out of the Tab order; it stays clickable by
            mouse. Its keyboard equivalents are Delete on the focused tab (above), the palette's
            "Close tab", and Ctrl/Cmd+W (`useWorkspaceShortcuts.ts`) where the host doesn't claim that key first. */}
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
      {/* Every open tab in one list with a filter (#625), also the ones scrolled out of view. */}
      <button
        ref={listButtonRef}
        type="button"
        className={styles.addButton}
        aria-label="List open tabs"
        title="List open tabs"
        aria-haspopup="dialog"
        aria-expanded={tabList !== null}
        onClick={(event) => {
          if (tabList !== null) {
            setTabList(null);
            return;
          }
          const rect = event.currentTarget.getBoundingClientRect();
          setTabList({ x: rect.right, y: rect.bottom });
        }}
      >
        <ChevronDown size={14} aria-hidden="true" />
      </button>
      {tabList !== null && (
        <TabList
          x={tabList.x}
          y={tabList.y}
          openRepos={openRepos}
          activePath={activePath}
          workspaceNames={workspaceNames}
          changedPaths={changedPaths}
          onSwitchTo={onSwitchTo}
          onClose={() => setTabList(null)}
          trigger={listButtonRef}
        />
      )}
      {/* Outside the tablist: a tablist may only own tabs. */}
      {menu !== null && tabMenuItems !== undefined && (
        <ContextMenu x={menu.x} y={menu.y} items={tabMenuItems(menu.path)} onClose={() => setMenu(null)} />
      )}
    </div>
  );
}
