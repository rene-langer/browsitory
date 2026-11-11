import { X } from "lucide-react";
import { useEffect, useState } from "react";
import type { RepoClient, Workspace } from "../ipc/RepoClient";
import { shortcutLabel } from "../lib/keybindings";
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
}) {
  const [recentRepos, setRecentRepos] = useState<string[]>([]);
  // Recent repositories whose folder is gone (#690): marked, still listed, and removable.
  const [missingRepos, setMissingRepos] = useState<ReadonlySet<string>>(new Set());
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [creatingWorkspace, setCreatingWorkspace] = useState(false);
  const [editingWorkspace, setEditingWorkspace] = useState<Workspace | null>(null);
  const [deleteConfirmation, setDeleteConfirmation] = useState<Workspace | null>(null);

  useEffect(() => {
    client
      .listRecentRepos()
      .then(setRecentRepos)
      .catch((err: unknown) => setError(String(err)));
    // Only a mark: if the check fails, every folder just looks present.
    client.listMissingRecentRepos().then(
      (paths) => setMissingRepos(new Set(paths)),
      () => {},
    );
  }, [client]);

  const removeRecentRepo = (path: string) => {
    client.removeRecentRepo(path).then(
      () => setRecentRepos((prev) => prev.filter((recent) => recent !== path)),
      (err: unknown) => setError(String(err)),
    );
  };

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
      <button onClick={handleOpenFolder}>Open folder</button>
      {onCloneRepository !== undefined && <button onClick={onCloneRepository}>Clone…</button>}
      {onNewRepository !== undefined && <button onClick={onNewRepository}>New repository…</button>}
    </>
  );
  const paletteKey = <kbd>{shortcutLabel("palette")}</kbd>;

  return (
    <div className={styles.picker}>
      <Panel title="Open a repository">
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
          <>
            <Toolbar>{openButtons}</Toolbar>
            <p className={styles.hint}>
              Tip: press {paletteKey} once a repository is open to search every command.
            </p>
          </>
        )}
        {error !== null && <InlineError message={error} onDismiss={() => setError(null)} />}
        {(recentRepos.length > 0 || workspaces.length > 0) && (
          <label className={styles.filter}>
            <span className={styles.srOnly}>Filter repositories and workspaces</span>
            <input
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
            <>
              <p className={styles.empty}>No recent repositories</p>
              <p className={styles.hint}>Open a folder that contains a Git repository to get started.</p>
            </>
          ) : shownRepos.length === 0 ? (
            noMatch
          ) : (
            <ul className={styles.list}>
              {shownRepos.map((path) => {
                const name = nameOf(path);
                const missing = missingRepos.has(path);
                return (
                  <ListRow key={path} className={styles.recentRow}>
                    <button
                      type="button"
                      className={missing ? `${styles.repoRow} ${styles.missing}` : styles.repoRow}
                      onClick={() => onOpenRepo(path)}
                    >
                      <span className={styles.repoName}>
                        {name}
                        {missing && (
                          <>
                            {" "}
                            <span className={styles.missingBadge}>Folder not found</span>
                          </>
                        )}
                      </span>
                      <span className={styles.repoPath}>{path}</span>
                    </button>
                    <button
                      type="button"
                      className={styles.removeButton}
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
        {deleteConfirmation !== null && (
          <ConfirmDialog
            ariaLabel={`Delete workspace ${deleteConfirmation.name}`}
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
