import { Pin, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { RepoClient, Workspace } from "../ipc/RepoClient";
import { shortcutLabel } from "../lib/keybindings";
import { useKeymap } from "../lib/keymap";
import { repoNameOf } from "../lib/repoName";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import { WorkspaceEditor } from "./WorkspaceEditor";
import styles from "./RepoPicker.module.css";

export function RepoPicker({
  client,
  onOpenRepo,
  onOpenWorkspace,
  workspaces,
  aliases,
  workspacesLoading,
  workspacesError,
  onDismissWorkspacesError,
  onCreateWorkspace,
  onEditWorkspace,
  onDeleteWorkspace,
  onCloneRepository,
  onNewRepository,
  welcome,
  onClose,
  openPaths,
  onSwitchToTab,
}: {
  client: RepoClient;
  onOpenRepo: (path: string) => void;
  onOpenWorkspace: (workspace: Workspace) => void;
  workspaces: Workspace[];
  /** Tab names by repository path (#625): shown in place of the folder name. */
  aliases?: ReadonlyMap<string, string>;
  workspacesLoading: boolean;
  workspacesError: string | null;
  onDismissWorkspacesError: () => void;
  onCreateWorkspace: (name: string, root: string, members: string[]) => Promise<string>;
  onEditWorkspace: (id: string, name: string, members: string[]) => Promise<void>;
  onDeleteWorkspace: (id: string) => Promise<void>;
  onCloneRepository?: () => void;
  onNewRepository?: () => void;
  /** Shows the welcome on top (#688): the first launch, until dismissed or a repository opens. */
  welcome?: { onDismiss: () => void; onOpenUserGuide: () => void };
  /**
   * Opened over a repository (the tab bar's "+", #1013): an "Open repository" dialog header with
   * a close button, and no palette tip.
   */
  onClose?: () => void;
  /** Repositories open as tabs (#1013): marked, and a click switches to their tab. */
  openPaths?: ReadonlySet<string>;
  onSwitchToTab?: (path: string) => void;
}) {
  useKeymap();
  const [recentRepos, setRecentRepos] = useState<string[]>([]);
  // The first list came in (or failed): the picker's controls are all there.
  const [loaded, setLoaded] = useState(false);
  // Recent repositories whose folder is gone (#690): marked, still listed, and removable.
  const [missingRepos, setMissingRepos] = useState<ReadonlySet<string>>(new Set());
  // Pinned repositories (#766): listed first by `listRecentRepos`, marked here.
  const [pinnedRepos, setPinnedRepos] = useState<ReadonlySet<string>>(new Set());
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [creatingWorkspace, setCreatingWorkspace] = useState(false);
  const [editingWorkspace, setEditingWorkspace] = useState<Workspace | null>(null);
  const [deleteConfirmation, setDeleteConfirmation] = useState<Workspace | null>(null);

  // Only a mark: if the pinned list can't be read, every row just shows as unpinned.
  const loadPinned = useCallback(
    () =>
      client.listPinnedRepos().then(
        (paths) => setPinnedRepos(new Set(paths)),
        () => setPinnedRepos(new Set()),
      ),
    [client],
  );

  useEffect(() => {
    client
      .listRecentRepos()
      .then(setRecentRepos)
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setLoaded(true));
    void loadPinned();
    // Only a mark: if the check fails, every folder just looks present.
    client.listMissingRecentRepos().then(
      (paths) => setMissingRepos(new Set(paths)),
      () => {},
    );
  }, [client, loadPinned]);

  const removeRecentRepo = (path: string) => {
    client.removeRecentRepo(path).then(
      () => setRecentRepos((prev) => prev.filter((recent) => recent !== path)),
      (err: unknown) => setError(String(err)),
    );
  };

  // Pinning reorders the list (pinned first, an unpinned one back on top), so both lists are read
  // again rather than reordered here.
  const setPinned = (path: string, pinned: boolean) => {
    (pinned ? client.pinRecentRepo(path) : client.unpinRecentRepo(path))
      .then(() => Promise.all([client.listRecentRepos().then(setRecentRepos), loadPinned()]))
      .catch((err: unknown) => setError(String(err)));
  };

  // Locate… (#998): the folder of a missing recent repository moved. Its new place replaces the
  // entry (pinned again if the old one was) and opens.
  const locate = (path: string) => {
    client
      .pickRepoFolder()
      .then(async (picked) => {
        if (picked === null) return;
        await client.removeRecentRepo(path);
        if (pinnedRepos.has(path)) await client.pinRecentRepo(picked);
        setRecentRepos((prev) => prev.filter((recent) => recent !== path));
        onOpenRepo(picked);
      })
      .catch((err: unknown) => setError(String(err)));
  };

  // Keyboard focus starts in the filter, or on Open repository… when there is nothing to filter
  // (#1029), not in the title bar. Once, and only if focus is nowhere yet or on the picker's own
  // first control (the dialog's ×): never taken from something the user moved it to.
  const rootRef = useRef<HTMLDivElement>(null);
  const filterRef = useRef<HTMLInputElement>(null);
  const openButtonRef = useRef<HTMLButtonElement>(null);
  const focusedOnce = useRef(false);
  useEffect(() => {
    if (!loaded || focusedOnce.current) return;
    focusedOnce.current = true;
    const active = document.activeElement;
    const unclaimed = active === null || active === document.body || rootRef.current?.contains(active) === true;
    if (unclaimed) (filterRef.current ?? openButtonRef.current)?.focus();
  }, [loaded]);

  const nameOf = (path: string) => aliases?.get(path) ?? repoNameOf(path);
  const query = filter.trim().toLowerCase();
  const matches = (...texts: string[]) => query === "" || texts.some((text) => text.toLowerCase().includes(query));
  const shownRepos = recentRepos.filter((path) => matches(nameOf(path), path));
  const shownWorkspaces = workspaces.filter((workspace) => matches(workspace.name, workspace.rootPath));
  const noMatch = <p className={styles.empty}>Nothing matches “{filter.trim()}”.</p>;

  const handleOpenFolder = () => {
    client
      .pickRepoFolder()
      .then((path) => {
        if (path !== null) {
          onOpenRepo(path);
        }
      })
      .catch((err: unknown) => setError(String(err)));
  };

  if (creatingWorkspace) {
    return (
      <WorkspaceEditor
        client={client}
        aliases={aliases}
        onSave={(name, root, members) =>
          onCreateWorkspace(name, root, members).then((id) => {
            onOpenWorkspace({ id, name, rootPath: root, memberPaths: members });
            setCreatingWorkspace(false);
          })
        }
        onCancel={() => setCreatingWorkspace(false)}
      />
    );
  }

  if (editingWorkspace !== null) {
    return (
      <WorkspaceEditor
        client={client}
        aliases={aliases}
        existing={editingWorkspace}
        onSave={(name, _root, members) =>
          onEditWorkspace(editingWorkspace.id, name, members).then(() => setEditingWorkspace(null))
        }
        onCancel={() => setEditingWorkspace(null)}
      />
    );
  }

  const openButtons = (
    <>
      <button ref={openButtonRef} onClick={handleOpenFolder}>
        Open repository…
      </button>
      {onCloneRepository !== undefined && <button onClick={onCloneRepository}>Clone…</button>}
      {onNewRepository !== undefined && <button onClick={onNewRepository}>New repository…</button>}
    </>
  );
  const paletteLabel = shortcutLabel("palette");
  // Without a key for it (#898) the header's Commands button opens the palette.
  const paletteKey = paletteLabel === "" ? "Commands" : <kbd>{paletteLabel}</kbd>;

  // Over a repository the picker is a dialog (#1013): its own header with ×, no card around it.
  // Kept to this header so it can move onto the shared dialog header later.
  const inDialog = onClose !== undefined;

  return (
    <div ref={rootRef} className={inDialog ? styles.inDialog : styles.picker}>
      <Panel
        title={inDialog ? "Open repository" : "Open a repository"}
        actions={
          inDialog ? (
            <button type="button" className={styles.closeButton} aria-label="Close" title="Close" onClick={onClose}>
              <X size={16} aria-hidden="true" />
            </button>
          ) : undefined
        }
      >
        {welcome !== undefined ? (
          <section className={styles.welcome} aria-labelledby="repo-picker-welcome">
            <h3 id="repo-picker-welcome" className={styles.welcomeTitle}>
              Welcome to Browsitory
            </h3>
            <p className={styles.welcomeText}>
              Open a folder that holds a Git repository, clone one, or start a new one. Each repository opens in
              its own tab.
            </p>
            <Toolbar aria-label="Get started">{openButtons}</Toolbar>
            <p className={styles.hint}>
              Once a repository is open, press {paletteKey} to search every command.
            </p>
            <Toolbar>
              <button onClick={welcome.onOpenUserGuide}>User guide</button>
              <button onClick={welcome.onDismiss}>Dismiss</button>
            </Toolbar>
          </section>
        ) : (
          <Toolbar>{openButtons}</Toolbar>
        )}
        {error !== null && <InlineError message={error} onDismiss={() => setError(null)} />}
        {(recentRepos.length > 0 || workspaces.length > 0) && (
          <label className={styles.filter}>
            <span className={styles.srOnly}>Filter repositories and workspaces</span>
            <input
              ref={filterRef}
              type="search"
              placeholder="Filter repositories and workspaces"
              value={filter}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setFilter(event.target.value)}
            />
          </label>
        )}
        <section className={styles.recent} aria-label="Recent repositories">
          <h3 className={styles.sectionTitle}>Recent repositories</h3>
          {recentRepos.length === 0 ? (
            // Said once the list is read, not while it loads.
            loaded && (
              <>
                <p className={styles.empty}>No recent repositories</p>
                <p className={styles.hint}>Open a folder that contains a Git repository to get started.</p>
              </>
            )
          ) : shownRepos.length === 0 ? (
            noMatch
          ) : (
            <ul className={styles.list}>
              {shownRepos.map((path) => {
                const name = nameOf(path);
                const missing = missingRepos.has(path);
                const pinned = pinnedRepos.has(path);
                const openInTab = openPaths?.has(path) === true && onSwitchToTab !== undefined;
                return (
                  <ListRow key={path} className={styles.recentRow}>
                    {missing ? (
                      // Opening it could only fail (#998): not a button; Locate… or Remove it.
                      <span className={`${styles.repoRow} ${styles.missing}`}>
                        <span className={styles.repoName}>
                          {name} <span className={styles.missingBadge}>Folder not found</span>
                        </span>
                        <span className={styles.repoPath}>{path}</span>
                      </span>
                    ) : (
                      <button
                        type="button"
                        className={styles.repoRow}
                        onClick={() => (openInTab ? onSwitchToTab(path) : onOpenRepo(path))}
                      >
                        <span className={styles.repoName}>{name}</span>
                        <span className={styles.repoPath}>{path}</span>
                        {openInTab && <span className={styles.openBadge}>Open in a tab</span>}
                      </button>
                    )}
                    {missing ? (
                      <button
                        type="button"
                        className={styles.locate}
                        aria-label={`Locate ${name}…`}
                        title="Pick the folder's new place"
                        onClick={() => locate(path)}
                      >
                        Locate…
                      </button>
                    ) : (
                      <button
                        type="button"
                        className={pinned ? `${styles.rowButton} ${styles.pinned}` : styles.rowButton}
                        aria-label={pinned ? `Unpin ${name}` : `Pin ${name}`}
                        aria-pressed={pinned}
                        title={pinned ? "Unpin: list it with the recent repositories again" : "Pin to the top of the list"}
                        onClick={() => setPinned(path, !pinned)}
                      >
                        <Pin size={14} aria-hidden="true" fill={pinned ? "currentColor" : "none"} />
                      </button>
                    )}
                    <button
                      type="button"
                      className={styles.rowButton}
                      aria-label={`Remove ${name} from recent repositories`}
                      title="Remove from recent repositories"
                      onClick={() => removeRecentRepo(path)}
                    >
                      <X size={14} aria-hidden="true" />
                    </button>
                  </ListRow>
                );
              })}
            </ul>
          )}
        </section>
        <section className={styles.workspaces} aria-label="Workspaces">
          <div className={styles.sectionHeader}>
            <h3 className={styles.sectionTitle}>Workspaces</h3>
            <button onClick={() => setCreatingWorkspace(true)}>New workspace…</button>
          </div>
          <p className={styles.hint}>
            A workspace is a saved group of repositories you can open together: pick a folder, then the
            repositories in it.
          </p>
          <div>
            {workspacesError !== null && (
              <InlineError message={workspacesError} onDismiss={onDismissWorkspacesError} />
            )}
            {!workspacesLoading && workspaces.length === 0 ? (
              <p className={styles.empty}>No saved workspaces</p>
            ) : workspaces.length > 0 && shownWorkspaces.length === 0 ? (
              noMatch
            ) : (
              <ul className={styles.list}>
                {shownWorkspaces.map((workspace) => (
                  <ListRow key={workspace.id}>
                    <span title={workspace.rootPath}>{workspace.name}</span>
                    <Toolbar>
                      <button
                        onClick={(event) => {
                          event.stopPropagation();
                          onOpenWorkspace(workspace);
                        }}
                      >
                        Open all
                      </button>
                      <button
                        onClick={(event) => {
                          event.stopPropagation();
                          setEditingWorkspace(workspace);
                        }}
                      >
                        Edit
                      </button>
                      <button
                        onClick={(event) => {
                          event.stopPropagation();
                          setDeleteConfirmation(workspace);
                        }}
                      >
                        Delete {workspace.name}
                      </button>
                    </Toolbar>
                  </ListRow>
                ))}
              </ul>
            )}
          </div>
        </section>
        {/* Below the lists (#1013): the welcome has its own, and over a repository it's moot. */}
        {!inDialog && welcome === undefined && (
          <p className={styles.hint}>Tip: press {paletteKey} once a repository is open to search every command.</p>
        )}
        {deleteConfirmation !== null && (
          <ConfirmDialog
            title={`Delete workspace ${deleteConfirmation.name}`}
            message={
              <p>
                Delete workspace {deleteConfirmation.name}? Its member repos stay open if currently open; only the
                saved workspace is removed.
              </p>
            }
            confirmLabel="Delete workspace"
            onConfirm={() =>
              void onDeleteWorkspace(deleteConfirmation.id).then(() => setDeleteConfirmation(null))
            }
            onCancel={() => setDeleteConfirmation(null)}
          />
        )}
      </Panel>
    </div>
  );
}
